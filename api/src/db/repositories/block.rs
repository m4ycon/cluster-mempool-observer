use super::RepoResult;
use crate::db::instrument::query;
use crate::db::models::NewBlock;
use crate::db::pool::DbPool;
use crate::db::schema::blocks;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use time::OffsetDateTime;

const REPO_LABEL: &str = "block";

#[derive(Clone)]
pub struct BlockRepository {
    pool: DbPool,
}

impl BlockRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, block: &NewBlock) -> RepoResult<usize> {
        query(&self.pool, REPO_LABEL, "insert", async |conn| {
            diesel::insert_into(blocks::table)
                .values(block)
                .on_conflict(blocks::hash)
                .do_nothing()
                .execute(conn)
                .await
        })
        .await
    }

    pub async fn latest_height(&self) -> RepoResult<Option<i64>> {
        query(&self.pool, REPO_LABEL, "latest_height", async |conn| {
            blocks::table
                .select(diesel::dsl::max(blocks::height))
                .first::<Option<i64>>(conn)
                .await
        })
        .await
    }

    pub async fn latest(&self) -> RepoResult<Option<(i64, OffsetDateTime)>> {
        query(&self.pool, REPO_LABEL, "latest", async |conn| {
            blocks::table
                .order(blocks::height.desc())
                .select((blocks::height, blocks::mined_at))
                .first::<(i64, OffsetDateTime)>(conn)
                .await
                .optional()
        })
        .await
    }
}
