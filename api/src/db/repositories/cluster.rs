use super::RepoResult;
use crate::db::models::{Cluster, NewCluster};
use crate::db::pool::DbPool;
use crate::db::schema::clusters;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

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
}
