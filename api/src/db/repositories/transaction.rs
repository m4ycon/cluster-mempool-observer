use super::RepoResult;
use crate::db::models::NewTransaction;
use crate::db::pool::DbPool;
use crate::db::schema::transactions;
use diesel::prelude::*;
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
}
