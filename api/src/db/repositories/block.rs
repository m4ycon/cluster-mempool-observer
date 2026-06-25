use super::RepoResult;
use crate::db::models::NewBlock;
use crate::db::pool::DbPool;
use crate::db::schema::blocks;
use diesel_async::RunQueryDsl;

#[derive(Clone)]
pub struct BlockRepository {
    pool: DbPool,
}

impl BlockRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, block: &NewBlock) -> RepoResult<usize> {
        let mut conn = self.pool.get().await?;
        let inserted = diesel::insert_into(blocks::table)
            .values(block)
            .on_conflict(blocks::hash)
            .do_nothing()
            .execute(&mut conn)
            .await?;
        Ok(inserted)
    }
}
