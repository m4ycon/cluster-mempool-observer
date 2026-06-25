use super::RepoResult;
use crate::db::models::NewBlock;
use crate::db::pool::DbPool;
use crate::db::schema::blocks;
use diesel::prelude::*;
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

    pub async fn latest_height(&self) -> RepoResult<Option<i64>> {
        let mut conn = self.pool.get().await?;
        let height = blocks::table
            .select(diesel::dsl::max(blocks::height))
            .first::<Option<i64>>(&mut conn)
            .await?;
        Ok(height)
    }
}
