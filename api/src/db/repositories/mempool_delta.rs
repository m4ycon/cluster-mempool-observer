use super::{MEMPOOL_DELTA_INSERT_CHUNK_SIZE, RepoResult};
use crate::db::instrument::query;
use crate::db::models::{DeltaDirection, DeltaReason, NewMempoolDelta};
use crate::db::pool::DbPool;
use crate::db::schema::mempool_deltas;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Timestamptz};
use diesel_async::RunQueryDsl;
use std::collections::HashSet;
use time::{Duration, OffsetDateTime};

/// Replay horizon for snapshot reconstruction. Bitcoin Core `-mempoolexpiry` defaults to 14 days.
const SNAPSHOT_REPLAY_WINDOW: Duration = Duration::days(15);

const REPO_LABEL: &str = "mempool_delta";

#[derive(QueryableByName)]
struct DeltaCounts {
    #[diesel(sql_type = BigInt)]
    added_txs: i64,
    #[diesel(sql_type = BigInt)]
    confirmed_txs: i64,
    #[diesel(sql_type = BigInt)]
    evicted_txs: i64,
}

#[derive(Clone)]
pub struct MempoolDeltaRepository {
    pool: DbPool,
}

impl MempoolDeltaRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert_many(&self, deltas: &[NewMempoolDelta]) -> RepoResult<usize> {
        if deltas.is_empty() {
            return Ok(0);
        }
        query(&self.pool, REPO_LABEL, "insert_many", async |conn| {
            let mut inserted = 0;
            for chunk in deltas.chunks(MEMPOOL_DELTA_INSERT_CHUNK_SIZE) {
                inserted += diesel::insert_into(mempool_deltas::table)
                    .values(chunk)
                    .execute(conn)
                    .await?;
            }
            Ok(inserted)
        })
        .await
    }

    pub async fn count(&self) -> RepoResult<i64> {
        query(&self.pool, REPO_LABEL, "count", async |conn| {
            mempool_deltas::table.count().get_result(conn).await
        })
        .await
    }

    pub async fn count_adds_since(&self, cutoff: OffsetDateTime) -> RepoResult<i64> {
        let add_reasons: Vec<DeltaReason> =
            DeltaReason::with_direction(DeltaDirection::Add).collect();
        query(&self.pool, REPO_LABEL, "count_adds_since", async |conn| {
            mempool_deltas::table
                .filter(mempool_deltas::created_at.ge(cutoff))
                .filter(mempool_deltas::reason.eq_any(add_reasons))
                .count()
                .get_result(conn)
                .await
        })
        .await
    }

    /// `mempool_deltas` rows in `(from, to]`, split by reason. Half-open so
    /// consecutive windows neither double-count nor drop a row landing exactly
    /// on a boundary.
    pub async fn count_deltas_in(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> RepoResult<(i64, i64, i64)> {
        let sql = "
            SELECT
                count(*) FILTER (WHERE reason = 'add_mempool') AS added_txs,
                count(*) FILTER (WHERE reason = 'remove_confirmed') AS confirmed_txs,
                count(*) FILTER (WHERE reason = 'remove_evicted') AS evicted_txs
            FROM mempool_deltas
            WHERE created_at > $1 AND created_at <= $2
        ";

        let counts: DeltaCounts = query(&self.pool, REPO_LABEL, "count_deltas_in", async |conn| {
            diesel::sql_query(sql)
                .bind::<Timestamptz, _>(from)
                .bind::<Timestamptz, _>(to)
                .get_result(conn)
                .await
        })
        .await?;

        Ok((counts.added_txs, counts.confirmed_txs, counts.evicted_txs))
    }

    pub async fn reconstruct_snapshot(&self) -> RepoResult<HashSet<String>> {
        let cutoff = OffsetDateTime::now_utc() - SNAPSHOT_REPLAY_WINDOW;
        // Replay happens after `query` returns, so the connection goes back to
        // the pool before the fold rather than being held across it.
        let rows: Vec<(String, DeltaReason)> = query(
            &self.pool,
            REPO_LABEL,
            "reconstruct_snapshot",
            async |conn| {
                mempool_deltas::table
                    .filter(mempool_deltas::created_at.ge(cutoff))
                    .order(mempool_deltas::id.asc())
                    .select((mempool_deltas::txid, mempool_deltas::reason))
                    .load(conn)
                    .await
            },
        )
        .await?;

        let mut snapshot = HashSet::new();
        for (txid, reason) in rows {
            match reason.direction() {
                DeltaDirection::Add => {
                    snapshot.insert(txid);
                }
                DeltaDirection::Remove => {
                    snapshot.remove(&txid);
                }
            }
        }
        Ok(snapshot)
    }
}
