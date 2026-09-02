use super::RepoResult;
use crate::db::instrument::query;
use crate::db::models::{NewTransaction, Transaction};
use crate::db::pool::DbPool;
use crate::db::schema::transactions;
use diesel::prelude::*;
use diesel::upsert::excluded;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, RunQueryDsl};

const REPO_LABEL: &str = "transaction";

/// Postgres caps a statement at 65535 bind params; `NewTransaction` has 9 columns.
pub const INSERT_CHUNK_SIZE: usize = 1000;

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

    pub async fn insert_many(&self, txs: &[NewTransaction]) -> RepoResult<usize> {
        if txs.is_empty() {
            return Ok(0);
        }
        query(&self.pool, REPO_LABEL, "insert_many", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                async move {
                    let mut inserted = 0;
                    for chunk in txs.chunks(INSERT_CHUNK_SIZE) {
                        inserted += diesel::insert_into(transactions::table)
                            .values(chunk)
                            .on_conflict(transactions::txid)
                            .do_nothing()
                            .execute(conn)
                            .await?;
                    }
                    Ok(inserted)
                }
                .scope_boxed()
            })
            .await
        })
        .await
    }

    pub async fn insert_or_confirm_many(&self, txs: &[NewTransaction]) -> RepoResult<usize> {
        query(
            &self.pool,
            REPO_LABEL,
            "insert_or_confirm_many",
            async |conn| {
                diesel::insert_into(transactions::table)
                    .values(txs)
                    .on_conflict(transactions::txid)
                    .do_update()
                    .set((
                        transactions::confirmed_at.eq(excluded(transactions::confirmed_at)),
                        transactions::fee.eq(excluded(transactions::fee)),
                        transactions::vsize.eq(excluded(transactions::vsize)),
                        transactions::confirmed_at_block
                            .eq(excluded(transactions::confirmed_at_block)),
                        transactions::hollow.eq(excluded(transactions::hollow)),
                        transactions::input_txids.eq(excluded(transactions::input_txids)),
                    ))
                    .execute(conn)
                    .await
            },
        )
        .await
    }

    /// Sums stored fee (NULL as 0) and vsize over the given txids.
    pub async fn get_fee_vsize_totals(&self, txids: &[String]) -> RepoResult<(i64, i64)> {
        let rows: Vec<(Option<i64>, i64)> = query(
            &self.pool,
            REPO_LABEL,
            "get_fee_vsize_totals",
            async |conn| {
                transactions::table
                    .filter(transactions::txid.eq_any(txids))
                    .select((transactions::fee, transactions::vsize))
                    .load(conn)
                    .await
            },
        )
        .await?;
        Ok(rows
            .into_iter()
            .fold((0, 0), |(fee_sum, vsize_sum), (fee, vsize)| {
                (fee_sum + fee.unwrap_or(0), vsize_sum + vsize)
            }))
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

    /// Fills in a row from a fetched tx, clearing `hollow`.
    ///
    /// Mirrors `NewTransaction::needs_backfill`, so nothing is queued
    /// that this write would then refuse.
    pub async fn backfill_from_fetch(
        &self,
        txid: &str,
        input_txids: &[String],
        vsize: i64,
    ) -> RepoResult<usize> {
        query(
            &self.pool,
            REPO_LABEL,
            "backfill_from_fetch",
            async |conn| {
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
                        transactions::hollow.eq(false),
                    ))
                    .execute(conn)
                    .await
            },
        )
        .await
    }
}
