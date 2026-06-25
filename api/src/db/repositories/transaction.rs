use super::RepoResult;
use crate::db::models::NewTransaction;
use crate::db::pool::DbPool;
use crate::db::schema::transactions;
use diesel::prelude::*;
use diesel::upsert::excluded;
use diesel_async::RunQueryDsl;

#[derive(Clone)]
pub struct TransactionRepository {
    pool: DbPool,
}

impl TransactionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn existing_txids(&self, ids: &[String]) -> RepoResult<Vec<String>> {
        let mut conn = self.pool.get().await?;
        let found = transactions::table
            .filter(transactions::txid.eq_any(ids))
            .select(transactions::txid)
            .load(&mut conn)
            .await?;
        Ok(found)
    }

    pub async fn insert(&self, tx: &NewTransaction) -> RepoResult<usize> {
        let mut conn = self.pool.get().await?;
        let inserted = diesel::insert_into(transactions::table)
            .values(tx)
            .on_conflict(transactions::txid)
            .do_nothing()
            .execute(&mut conn)
            .await?;
        Ok(inserted)
    }

    pub async fn insert_or_confirm_many(&self, txs: &[NewTransaction]) -> RepoResult<usize> {
        let mut conn = self.pool.get().await?;
        let inserted = diesel::insert_into(transactions::table)
            .values(txs)
            .on_conflict(transactions::txid)
            .do_update()
            .set((
                transactions::confirmed_at.eq(excluded(transactions::confirmed_at)),
                transactions::fee.eq(excluded(transactions::fee)),
            ))
            .execute(&mut conn)
            .await?;
        Ok(inserted)
    }

    pub async fn get_cluster_ids_by_txids(&self, txids: &[String]) -> RepoResult<Vec<i64>> {
        let mut conn = self.pool.get().await?;
        let ids = transactions::table
            .filter(transactions::txid.eq_any(txids))
            .filter(transactions::cluster_id.is_not_null())
            .select(transactions::cluster_id)
            .distinct()
            .load::<Option<i64>>(&mut conn)
            .await?;
        Ok(ids.into_iter().flatten().collect())
    }

    pub async fn set_cluster_id(&self, txids: &[String], cluster_id: i64) -> RepoResult<usize> {
        let mut conn = self.pool.get().await?;
        let updated = diesel::update(transactions::table)
            .filter(transactions::txid.eq_any(txids))
            .set(transactions::cluster_id.eq(cluster_id))
            .execute(&mut conn)
            .await?;
        Ok(updated)
    }

    pub async fn clear_cluster_id(&self, txids: &[String]) -> RepoResult<usize> {
        let mut conn = self.pool.get().await?;
        let updated = diesel::update(transactions::table)
            .filter(transactions::txid.eq_any(txids))
            .set(transactions::cluster_id.eq(None::<i64>))
            .execute(&mut conn)
            .await?;
        Ok(updated)
    }
}
