use super::RepoResult;
use crate::db::instrument::query;
use crate::db::models::NewTransaction;
use crate::db::pool::DbPool;
use crate::db::schema::transactions;
use diesel::prelude::*;
use diesel::upsert::excluded;
use diesel_async::RunQueryDsl;

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
}
