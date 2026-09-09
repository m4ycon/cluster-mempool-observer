use super::{MEMPOOL_DELTA_INSERT_CHUNK_SIZE, RepoResult, TRANSACTION_INSERT_CHUNK_SIZE};
use crate::db::instrument::query;
use crate::db::models::{DeltaDirection, DeltaReason, NewMempoolDelta, NewTransaction};
use crate::db::pool::DbPool;
use crate::db::schema::{mempool_deltas, transactions};
use diesel::prelude::*;
use diesel::upsert::excluded;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use shared::snapshot::{JournalEntry, distinct_txids};
use std::collections::HashSet;

const REPO_LABEL: &str = "mempool_ledger";

pub struct FlushOutcome {
    pub new_txids: Vec<String>,
    pub evicted: Vec<String>,
}

#[derive(Clone)]
pub struct MempoolLedgerRepository {
    pool: DbPool,
}

impl MempoolLedgerRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Writes one journal batch: an `mempool_deltas` row per entry, plus hollow
    /// `transactions` rows for adds not already stored.
    pub async fn write_batch(&self, entries: &[JournalEntry]) -> RepoResult<FlushOutcome> {
        if entries.is_empty() {
            return Ok(FlushOutcome {
                new_txids: Vec::new(),
                evicted: Vec::new(),
            });
        }

        query(&self.pool, REPO_LABEL, "write_batch", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                async move {
                    let evicted = write_deltas(conn, entries).await?;
                    let new_txids = insert_hollow_transactions(conn, entries).await?;

                    Ok(FlushOutcome { new_txids, evicted })
                }
                .scope_boxed()
            })
            .await
        })
        .await
    }
}

/// One `mempool_deltas` row per journal entry, each remove classified against
/// `transactions.confirmed_at`. Returns the txids written as `RemoveEvicted`.
async fn write_deltas(
    conn: &mut AsyncPgConnection,
    entries: &[JournalEntry],
) -> Result<Vec<String>, diesel::result::Error> {
    let remove_txids = distinct_txids(entries, DeltaDirection::Remove);

    let mut confirmed: HashSet<String> = HashSet::new();
    for chunk in remove_txids.chunks(MEMPOOL_DELTA_INSERT_CHUNK_SIZE) {
        let found: Vec<String> = transactions::table
            .filter(transactions::txid.eq_any(chunk))
            .filter(transactions::confirmed_at.is_not_null())
            .select(transactions::txid)
            .load(conn)
            .await?;
        confirmed.extend(found);
    }

    let mut evicted: HashSet<String> = HashSet::new();
    let mut rows: Vec<NewMempoolDelta> = Vec::with_capacity(entries.len());
    for entry in entries {
        let reason = match entry.direction {
            DeltaDirection::Add => DeltaReason::AddMempool,
            DeltaDirection::Remove if confirmed.contains(&entry.txid) => {
                DeltaReason::RemoveConfirmed
            }
            DeltaDirection::Remove => {
                evicted.insert(entry.txid.clone());
                DeltaReason::RemoveEvicted
            }
        };
        rows.push(NewMempoolDelta {
            txid: entry.txid.clone(),
            reason,
        });
    }

    for chunk in rows.chunks(MEMPOOL_DELTA_INSERT_CHUNK_SIZE) {
        diesel::insert_into(mempool_deltas::table)
            .values(chunk)
            .execute(conn)
            .await?;
    }

    Ok(evicted.into_iter().collect())
}

/// A hollow `transactions` row for every added txid with no row yet.
/// Returns those txids for post-backfill.
async fn insert_hollow_transactions(
    conn: &mut AsyncPgConnection,
    entries: &[JournalEntry],
) -> Result<Vec<String>, diesel::result::Error> {
    let add_txids = distinct_txids(entries, DeltaDirection::Add);

    let mut existing: HashSet<String> = HashSet::new();
    for chunk in add_txids.chunks(MEMPOOL_DELTA_INSERT_CHUNK_SIZE) {
        let found: Vec<String> = transactions::table
            .filter(transactions::txid.eq_any(chunk))
            .select(transactions::txid)
            .load(conn)
            .await?;
        existing.extend(found);
    }

    let new_txids: Vec<String> = add_txids
        .into_iter()
        .filter(|txid| !existing.contains(txid))
        .collect();

    let rows: Vec<NewTransaction> = new_txids
        .iter()
        .map(|txid| NewTransaction::hollow(txid))
        .collect();
    for chunk in rows.chunks(TRANSACTION_INSERT_CHUNK_SIZE) {
        let upsert = diesel::insert_into(transactions::table)
            .values(chunk)
            .on_conflict(transactions::txid)
            .do_update()
            .set((
                transactions::fee.eq(excluded(transactions::fee)),
                transactions::vsize.eq(excluded(transactions::vsize)),
                transactions::hollow.eq(excluded(transactions::hollow)),
                transactions::input_txids.eq(excluded(transactions::input_txids)),
            ));
        diesel::query_dsl::methods::FilterDsl::filter(upsert, transactions::hollow.eq(true))
            .execute(conn)
            .await?;
    }

    Ok(new_txids)
}
