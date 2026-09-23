use super::RepoResult;
use crate::db::instrument::query;
use crate::db::models::{NewBlock, NewTransaction};
use crate::db::pool::DbPool;
use crate::db::schema::{blocks, transactions};
use diesel::prelude::*;
use diesel::upsert::excluded;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
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

    pub async fn insert_with_transactions(
        &self,
        block: &NewBlock,
        txs: &[NewTransaction],
    ) -> RepoResult<()> {
        query(
            &self.pool,
            REPO_LABEL,
            "insert_with_transactions",
            async |conn| {
                conn.transaction::<_, diesel::result::Error, _>(|conn| {
                    async move {
                        diesel::insert_into(blocks::table)
                            .values(block)
                            .on_conflict(blocks::hash)
                            .do_nothing()
                            .execute(conn)
                            .await?;
                        insert_or_confirm_many(conn, txs).await?;
                        Ok(())
                    }
                    .scope_boxed()
                })
                .await
            },
        )
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

/// Inserts `txs`, or overwrites the confirmation and body of the ones already
/// stored.
async fn insert_or_confirm_many(
    conn: &mut AsyncPgConnection,
    txs: &[NewTransaction],
) -> QueryResult<usize> {
    let txs = NewTransaction::sorted_by_txid(txs);
    diesel::insert_into(transactions::table)
        .values(txs)
        .on_conflict(transactions::txid)
        .do_update()
        .set((
            transactions::confirmed_at.eq(excluded(transactions::confirmed_at)),
            transactions::fee.eq(excluded(transactions::fee)),
            transactions::vsize.eq(excluded(transactions::vsize)),
            transactions::confirmed_at_block.eq(excluded(transactions::confirmed_at_block)),
            transactions::hollow.eq(excluded(transactions::hollow)),
            transactions::input_txids.eq(excluded(transactions::input_txids)),
        ))
        .execute(conn)
        .await
}
