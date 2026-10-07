use super::RepoResult;
use crate::db::instrument::query;
use crate::db::models::{NewTransaction, StoredTxFill, Transaction};
use crate::db::pool::DbPool;
use crate::db::schema::{blocks, transactions};
use diesel::prelude::*;
use diesel::upsert::excluded;
use diesel_async::{AsyncConnection, RunQueryDsl};
use time::OffsetDateTime;

const REPO_LABEL: &str = "transaction";

#[derive(Clone)]
pub struct TransactionRepository {
    pool: DbPool,
}

impl TransactionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn existing_txids(&self, ids: &[String]) -> RepoResult<Vec<String>> {
        query(&self.pool, REPO_LABEL, "existing_txids", async |conn| {
            transactions::table
                .filter(transactions::txid.eq_any(ids))
                .select(transactions::txid)
                .load(conn)
                .await
        })
        .await
    }

    pub async fn find_by_txids(&self, txids: &[String]) -> RepoResult<Vec<Transaction>> {
        query(&self.pool, REPO_LABEL, "find_by_txids", async |conn| {
            transactions::table
                .filter(transactions::txid.eq_any(txids))
                .select(Transaction::as_select())
                .load(conn)
                .await
        })
        .await
    }

    pub async fn find_fills_by_txids(
        &self,
        txids: &[String],
    ) -> RepoResult<Vec<(String, StoredTxFill)>> {
        query(
            &self.pool,
            REPO_LABEL,
            "find_fills_by_txids",
            async |conn| {
                transactions::table
                    .filter(transactions::txid.eq_any(txids))
                    .select((
                        transactions::txid,
                        (
                            transactions::input_txids.is_not_null(),
                            transactions::vsize,
                            transactions::fee.is_not_null(),
                        ),
                    ))
                    .load(conn)
                    .await
            },
        )
        .await
    }

    /// The mined txs among `txids`, each with its block's height and time.
    pub async fn find_mined_by_txids(
        &self,
        txids: &[String],
    ) -> RepoResult<Vec<(Transaction, i64, OffsetDateTime)>> {
        query(
            &self.pool,
            REPO_LABEL,
            "find_mined_by_txids",
            async |conn| {
                transactions::table
                    .inner_join(blocks::table)
                    .filter(transactions::txid.eq_any(txids))
                    .select((Transaction::as_select(), blocks::height, blocks::mined_at))
                    .load(conn)
                    .await
            },
        )
        .await
    }

    pub async fn insert(&self, tx: &NewTransaction) -> RepoResult<usize> {
        query(&self.pool, REPO_LABEL, "insert", async |conn| {
            diesel::insert_into(transactions::table)
                .values(tx)
                .on_conflict(transactions::txid)
                .do_nothing()
                .execute(conn)
                .await
        })
        .await
    }

    /// Inserts `txs`. A txid already stored keeps its row, except that a fee it
    /// lacks is filled from the incoming one.
    pub async fn insert_many(&self, txs: &[NewTransaction]) -> RepoResult<usize> {
        if txs.is_empty() {
            return Ok(0);
        }
        let txs = NewTransaction::sorted_by_txid(txs);
        query(&self.pool, REPO_LABEL, "insert_many", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                let mut written = 0;
                for chunk in txs.chunks(super::TRANSACTION_INSERT_CHUNK_SIZE) {
                    let upsert = diesel::insert_into(transactions::table)
                        .values(chunk.to_vec())
                        .on_conflict(transactions::txid)
                        .do_update()
                        .set(transactions::fee.eq(excluded(transactions::fee)));
                    written += diesel::query_dsl::methods::FilterDsl::filter(
                        upsert,
                        transactions::fee
                            .is_null()
                            .and(excluded(transactions::fee).is_not_null()),
                    )
                    .execute(conn)
                    .await?;
                }
                Ok(written)
            })
            .await
        })
        .await
    }

    /// Reads the `cluster_id` back-link, a denormalization: not the source of truth for cluster identity.
    pub async fn get_cluster_ids_by_txids(&self, txids: &[String]) -> RepoResult<Vec<i64>> {
        let ids = query(
            &self.pool,
            REPO_LABEL,
            "get_cluster_ids_by_txids",
            async |conn| {
                transactions::table
                    .filter(transactions::txid.eq_any(txids))
                    .filter(transactions::cluster_id.is_not_null())
                    .select(transactions::cluster_id)
                    .distinct()
                    .load::<Option<i64>>(conn)
                    .await
            },
        )
        .await?;
        Ok(ids.into_iter().flatten().collect())
    }

    pub async fn set_cluster_id(&self, txids: &[String], cluster_id: i64) -> RepoResult<usize> {
        query(&self.pool, REPO_LABEL, "set_cluster_id", async |conn| {
            diesel::update(transactions::table)
                .filter(transactions::txid.eq_any(txids))
                .set(transactions::cluster_id.eq(cluster_id))
                .execute(conn)
                .await
        })
        .await
    }

    /// Fills the parents and vsize a `getrawtransaction` fetch supplies, then
    /// reports what the row still lacks. `None` when it already had both --
    /// the block path may fill a row between fetch and write.
    pub async fn backfill_raw(
        &self,
        txid: &str,
        input_txids: &[String],
        vsize: i64,
    ) -> RepoResult<Option<StoredTxFill>> {
        query(&self.pool, REPO_LABEL, "backfill_raw", async |conn| {
            diesel::update(transactions::table)
                .filter(transactions::txid.eq(txid))
                .filter(
                    transactions::input_txids
                        .is_null()
                        .or(transactions::vsize.eq(0)),
                )
                .set((
                    transactions::input_txids.eq(Some(input_txids)),
                    transactions::vsize.eq(vsize),
                ))
                .returning((
                    transactions::input_txids.is_not_null(),
                    transactions::vsize,
                    transactions::fee.is_not_null(),
                ))
                .get_result(conn)
                .await
                .optional()
        })
        .await
    }

    pub async fn backfill_fee(&self, txid: &str, fee: i64) -> RepoResult<Option<StoredTxFill>> {
        query(&self.pool, REPO_LABEL, "backfill_fee", async |conn| {
            diesel::update(transactions::table)
                .filter(transactions::txid.eq(txid))
                .filter(transactions::fee.is_null())
                .set(transactions::fee.eq(Some(fee)))
                .returning((
                    transactions::input_txids.is_not_null(),
                    transactions::vsize,
                    transactions::fee.is_not_null(),
                ))
                .get_result(conn)
                .await
                .optional()
        })
        .await
    }
}
