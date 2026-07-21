use super::RepoResult;
use crate::db::models::{DeltaDirection, DeltaReason, NewMempoolDelta};
use crate::db::pool::DbPool;
use crate::db::schema::{mempool_deltas, transactions};
use diesel::prelude::*;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, RunQueryDsl};
use std::collections::{HashMap, HashSet};
use time::{Duration, OffsetDateTime};

/// Replay horizon for snapshot reconstruction. Bitcoin Core `-mempoolexpiry` defaults to 14 days.
const SNAPSHOT_REPLAY_WINDOW: Duration = Duration::days(15);

/// Avoid inserting too many rows at once, which can cause performance issues or exceed database limits.
const INSERT_CHUNK_SIZE: usize = 10_000;

/// Used to lock the removal path
const REMOVES_LOCK_KEY: i64 = 0x6d70_6f6f_6c5f_726d; // "mpool_rm"

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
        let mut inserted = 0;
        for chunk in deltas.chunks(INSERT_CHUNK_SIZE) {
            inserted += diesel::insert_into(mempool_deltas::table)
                .values(chunk)
                .execute(&mut conn)
                .await?;
        }
        Ok(inserted)
    }

    /// Records one removal row per candidate txid that still has an unpaired
    /// `add_mempool` event: `remove_confirmed` when the tx is confirmed in
    /// `transactions`, `remove_evicted` otherwise. Candidates already paired
    /// (or never added) are skipped, so the block path and the delta persister
    /// can both offer the same txid without double-removing it.
    /// Returns the txids recorded as evicted, sorted.
    pub async fn record_removes_for_unpaired(
        &self,
        candidates: &[String],
    ) -> RepoResult<Vec<String>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let candidates: Vec<String> = candidates
            .iter()
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let mut conn = self.pool.get().await?;
        let evicted = conn
            .transaction::<_, diesel::result::Error, _>(|conn| {
                async move {
                    // Global lock to serialize removals, no problem as this has a low frequency.
                    // This will prevent double-removing the same txid if both the block path
                    // and the delta persister offer it at the same time.
                    diesel::sql_query("SELECT pg_advisory_xact_lock($1)")
                        .bind::<diesel::sql_types::BigInt, _>(REMOVES_LOCK_KEY)
                        .execute(conn)
                        .await?;

                    // net adds per candidate; absent txids were never added
                    let mut net: HashMap<String, i64> = HashMap::new();
                    for chunk in candidates.chunks(INSERT_CHUNK_SIZE) {
                        let rows: Vec<(String, DeltaReason)> = mempool_deltas::table
                            .filter(mempool_deltas::txid.eq_any(chunk))
                            .select((mempool_deltas::txid, mempool_deltas::reason))
                            .load(conn)
                            .await?;
                        for (txid, reason) in rows {
                            *net.entry(txid).or_insert(0) += match reason.direction() {
                                DeltaDirection::Add => 1,
                                DeltaDirection::Remove => -1,
                            };
                        }
                    }
                    let unpaired: Vec<String> = net
                        .into_iter()
                        .filter(|(_, n)| *n > 0)
                        .map(|(txid, _)| txid)
                        .collect();
                    if unpaired.is_empty() {
                        return Ok(Vec::new());
                    }

                    let mut confirmed: HashSet<String> = HashSet::new();
                    for chunk in unpaired.chunks(INSERT_CHUNK_SIZE) {
                        let rows: Vec<String> = transactions::table
                            .filter(transactions::txid.eq_any(chunk))
                            .filter(transactions::confirmed_at.is_not_null())
                            .select(transactions::txid)
                            .load(conn)
                            .await?;
                        confirmed.extend(rows);
                    }

                    let rows: Vec<NewMempoolDelta> = unpaired
                        .iter()
                        .map(|txid| NewMempoolDelta {
                            txid: txid.clone(),
                            reason: if confirmed.contains(txid) {
                                DeltaReason::RemoveConfirmed
                            } else {
                                DeltaReason::RemoveEvicted
                            },
                        })
                        .collect();
                    for chunk in rows.chunks(INSERT_CHUNK_SIZE) {
                        diesel::insert_into(mempool_deltas::table)
                            .values(chunk)
                            .execute(conn)
                            .await?;
                    }

                    let mut evicted: Vec<String> = unpaired
                        .into_iter()
                        .filter(|txid| !confirmed.contains(txid))
                        .collect();
                    evicted.sort();
                    Ok(evicted)
                }
                .scope_boxed()
            })
            .await?;
        Ok(evicted)
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
