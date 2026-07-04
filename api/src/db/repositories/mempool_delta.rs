use super::RepoResult;
use crate::db::models::{DeltaDirection, DeltaReason, NewMempoolDelta};
use crate::db::pool::DbPool;
use crate::db::schema::mempool_deltas;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::collections::HashSet;
use time::{Duration, OffsetDateTime};

/// Replay horizon for snapshot reconstruction. Bitcoin Core `-mempoolexpiry` defaults to 14 days.
const SNAPSHOT_REPLAY_WINDOW: Duration = Duration::days(15);

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
        let mut conn = self.pool.get().await?;
        let inserted = diesel::insert_into(mempool_deltas::table)
            .values(deltas)
            .execute(&mut conn)
            .await?;
        Ok(inserted)
    }

    pub async fn count(&self) -> RepoResult<i64> {
        let mut conn = self.pool.get().await?;
        let total = mempool_deltas::table.count().get_result(&mut conn).await?;
        Ok(total)
    }

    pub async fn reconstruct_snapshot(&self) -> RepoResult<HashSet<String>> {
        let mut conn = self.pool.get().await?;
        let cutoff = OffsetDateTime::now_utc() - SNAPSHOT_REPLAY_WINDOW;
        let rows: Vec<(String, DeltaReason)> = mempool_deltas::table
            .filter(mempool_deltas::created_at.ge(cutoff))
            .order(mempool_deltas::id.asc())
            .select((mempool_deltas::txid, mempool_deltas::reason))
            .load(&mut conn)
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
