use super::RepoResult;
use crate::db::models::NewMempoolDelta;
use crate::db::pool::DbPool;
use crate::db::schema::mempool_deltas;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

#[derive(Clone)]
pub struct MempoolDeltaRepository {
    pool: DbPool,
}

impl MempoolDeltaRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, delta: &NewMempoolDelta) -> RepoResult<usize> {
        let mut conn = self.pool.get().await?;
        let inserted = diesel::insert_into(mempool_deltas::table)
            .values(delta)
            .execute(&mut conn)
            .await?;
        Ok(inserted)
    }

    pub async fn count(&self) -> RepoResult<i64> {
        let mut conn = self.pool.get().await?;
        let total = mempool_deltas::table.count().get_result(&mut conn).await?;
        Ok(total)
    }
}
