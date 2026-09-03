use super::{MEMPOOL_DELTA_INSERT_CHUNK_SIZE, RepoResult, TRANSACTION_INSERT_CHUNK_SIZE};
use crate::db::instrument::query;
use crate::db::models::{DeltaReason, NewMempoolDelta, NewTransaction};
use crate::db::pool::DbPool;
use crate::db::schema::{mempool_deltas, transactions};
use diesel::prelude::*;
use diesel::query_dsl::methods::FilterDsl;
use diesel::upsert::excluded;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, RunQueryDsl};

const REPO_LABEL: &str = "mempool_admission";

#[derive(Clone)]
pub struct MempoolAdmissionRepository {
    pool: DbPool,
}

impl MempoolAdmissionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Admits `added` into the mempool: one `add_mempool` event per txid, plus
    /// a `transactions` row for each of `new_txs` (the subset of `added` not
    /// already stored).
    pub async fn admit(&self, added: &[String], new_txs: &[NewTransaction]) -> RepoResult<()> {
        if added.is_empty() {
            return Ok(());
        }

        let add_rows: Vec<NewMempoolDelta> = added
            .iter()
            .map(|txid| NewMempoolDelta {
                txid: txid.clone(),
                reason: DeltaReason::AddMempool,
            })
            .collect();

        query(&self.pool, REPO_LABEL, "admit", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                async move {
                    for chunk in add_rows.chunks(MEMPOOL_DELTA_INSERT_CHUNK_SIZE) {
                        diesel::insert_into(mempool_deltas::table)
                            .values(chunk)
                            .execute(conn)
                            .await?;
                    }

                    for chunk in new_txs.chunks(TRANSACTION_INSERT_CHUNK_SIZE) {
                        diesel::insert_into(transactions::table)
                            .values(chunk)
                            .on_conflict(transactions::txid)
                            .do_update()
                            .set((
                                transactions::fee.eq(excluded(transactions::fee)),
                                transactions::vsize.eq(excluded(transactions::vsize)),
                                transactions::hollow.eq(excluded(transactions::hollow)),
                                transactions::input_txids.eq(excluded(transactions::input_txids)),
                            ))
                            .filter(transactions::hollow.eq(true))
                            .execute(conn)
                            .await?;
                    }

                    Ok(())
                }
                .scope_boxed()
            })
            .await
        })
        .await
    }
}
