use super::RepoResult;
use crate::db::models::{Cluster, NewCluster};
use crate::db::pool::DbPool;
use crate::db::schema::clusters;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct ClusterRepository {
    pool: DbPool,
}

impl ClusterRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, cluster: &NewCluster) -> RepoResult<Cluster> {
        let mut conn = self.pool.get().await?;
        let inserted = diesel::insert_into(clusters::table)
            .values(cluster)
            .returning(Cluster::as_returning())
            .get_result(&mut conn)
            .await?;
        Ok(inserted)
    }

    pub async fn find_by_txid(&self, txid: &str) -> RepoResult<Option<Cluster>> {
        let mut conn = self.pool.get().await?;
        let found = clusters::table
            .filter(clusters::txids.contains(vec![txid]))
            .select(Cluster::as_select())
            .first(&mut conn)
            .await
            .optional()?;
        Ok(found)
    }

    pub async fn update(
        &self,
        id: i64,
        txids: &[String],
        total_vsize: i64,
        total_fee: i64,
    ) -> RepoResult<Cluster> {
        let mut conn = self.pool.get().await?;
        let updated = diesel::update(clusters::table.find(id))
            .set((
                clusters::txids.eq(txids),
                clusters::total_vsize.eq(total_vsize),
                clusters::total_fee.eq(total_fee),
            ))
            .returning(Cluster::as_returning())
            .get_result(&mut conn)
            .await?;
        Ok(updated)
    }

    pub async fn confirm(&self, id: i64, confirmed_at: OffsetDateTime) -> RepoResult<Cluster> {
        let mut conn = self.pool.get().await?;
        let updated = diesel::update(clusters::table.find(id))
            .set(clusters::confirmed_at.eq(confirmed_at))
            .returning(Cluster::as_returning())
            .get_result(&mut conn)
            .await?;
        Ok(updated)
    }

    pub async fn find_by_ids(&self, ids: &[i64]) -> RepoResult<Vec<Cluster>> {
        let mut conn = self.pool.get().await?;
        let rows = clusters::table
            .filter(clusters::id.eq_any(ids))
            .select(Cluster::as_select())
            .load(&mut conn)
            .await?;
        Ok(rows)
    }

    pub async fn find_active(&self) -> RepoResult<Vec<Cluster>> {
        let mut conn = self.pool.get().await?;
        let rows = clusters::table
            .filter(clusters::confirmed_at.is_null())
            .select(Cluster::as_select())
            .load(&mut conn)
            .await?;
        Ok(rows)
    }

    pub async fn count(&self) -> RepoResult<i64> {
        let mut conn = self.pool.get().await?;
        let total = clusters::table.count().get_result(&mut conn).await?;
        Ok(total)
    }

    pub async fn delete_many(&self, ids: &[i64]) -> RepoResult<usize> {
        let mut conn = self.pool.get().await?;
        let deleted = diesel::delete(clusters::table.filter(clusters::id.eq_any(ids)))
            .execute(&mut conn)
            .await?;
        Ok(deleted)
    }
}
