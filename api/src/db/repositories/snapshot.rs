use super::RepoResult;
use crate::db::instrument::query;
use crate::db::models::{MempoolSnapshotRow, NewMempoolSnapshotRow};
use crate::db::pool::DbPool;
use crate::db::schema::mempool_snapshots;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Timestamptz};
use diesel_async::RunQueryDsl;
use time::OffsetDateTime;

const REPO_LABEL: &str = "snapshot";

/// `SnapshotService::run`'s sampling cadence.
pub const NATIVE_RESOLUTION_SECS: i64 = 60;

#[derive(Clone)]
pub struct SnapshotRepository {
    pool: DbPool,
}

impl SnapshotRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, snapshot: &NewMempoolSnapshotRow) -> RepoResult<usize> {
        query(&self.pool, REPO_LABEL, "insert", async |conn| {
            diesel::insert_into(mempool_snapshots::table)
                .values(snapshot)
                .on_conflict(mempool_snapshots::sampled_at)
                .do_nothing()
                .execute(conn)
                .await
        })
        .await
    }

    /// Points in `[from, to]` ascending, at `resolution_secs` granularity.
    pub async fn range(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
        resolution_secs: i64,
    ) -> RepoResult<Vec<MempoolSnapshotRow>> {
        if resolution_secs == NATIVE_RESOLUTION_SECS {
            self.range_native(from, to).await
        } else {
            self.range_bucketed(from, to, resolution_secs).await
        }
    }

    async fn range_native(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> RepoResult<Vec<MempoolSnapshotRow>> {
        query(&self.pool, REPO_LABEL, "range_native", async |conn| {
            mempool_snapshots::table
                .filter(mempool_snapshots::sampled_at.ge(from))
                .filter(mempool_snapshots::sampled_at.le(to))
                .order(mempool_snapshots::sampled_at.asc())
                .select(MempoolSnapshotRow::as_select())
                .load(conn)
                .await
        })
        .await
    }

    async fn range_bucketed(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
        resolution_secs: i64,
    ) -> RepoResult<Vec<MempoolSnapshotRow>> {
        // Keeps only the last row per bucket (`DISTINCT ON` + `sampled_at DESC`), so
        // every point is a real observed sample rather than an average. The outer
        // SELECT re-sorts ascending, since `DISTINCT ON` must order by bucket first.
        let sql = "
            WITH bucketed AS (
                SELECT DISTINCT ON (bucket)
                    to_timestamp(floor(extract(epoch FROM sampled_at) / $1::float8) * $1::float8) AS bucket,
                    sampled_at, cluster_count, clustered_tx_count, mempool_tx_count, total_vsize, total_fee
                FROM mempool_snapshots
                WHERE sampled_at >= $2 AND sampled_at <= $3
                ORDER BY bucket, sampled_at DESC
            )
            SELECT sampled_at, cluster_count, clustered_tx_count, mempool_tx_count, total_vsize, total_fee
            FROM bucketed
            ORDER BY sampled_at ASC
        ";

        query(&self.pool, REPO_LABEL, "range_bucketed", async |conn| {
            diesel::sql_query(sql)
                .bind::<BigInt, _>(resolution_secs)
                .bind::<Timestamptz, _>(from)
                .bind::<Timestamptz, _>(to)
                .load::<MempoolSnapshotRow>(conn)
                .await
        })
        .await
    }
}
