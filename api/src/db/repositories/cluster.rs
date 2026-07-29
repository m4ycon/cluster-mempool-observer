use super::RepoResult;
use crate::db::instrument::query;
use crate::db::models::{Cluster, NewCluster};
use crate::db::pool::DbPool;
use crate::db::schema::clusters;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

const REPO_LABEL: &str = "cluster";

#[derive(Clone)]
pub struct ClusterRepository {
    pool: DbPool,
}

impl ClusterRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, cluster: &NewCluster) -> RepoResult<Cluster> {
        query(&self.pool, REPO_LABEL, "insert", async |conn| {
            diesel::insert_into(clusters::table)
                .values(cluster)
                .returning(Cluster::as_returning())
                .get_result(conn)
                .await
        })
        .await
    }

    pub async fn find_by_txid(&self, txid: &str) -> RepoResult<Option<Cluster>> {
        query(&self.pool, REPO_LABEL, "find_by_txid", async |conn| {
            clusters::table
                .filter(clusters::txids.contains(vec![txid]))
                .select(Cluster::as_select())
                .first(conn)
                .await
                .optional()
        })
        .await
    }

    pub async fn update(
        &self,
        id: i64,
        txids: &[String],
        total_vsize: i64,
        total_fee: i64,
    ) -> RepoResult<Cluster> {
        query(&self.pool, REPO_LABEL, "update", async |conn| {
            diesel::update(clusters::table.find(id))
                .set((
                    clusters::txids.eq(txids),
                    clusters::total_vsize.eq(total_vsize),
                    clusters::total_fee.eq(total_fee),
                ))
                .returning(Cluster::as_returning())
                .get_result(conn)
                .await
        })
        .await
    }

    pub async fn find_by_ids(&self, ids: &[i64]) -> RepoResult<Vec<Cluster>> {
        query(&self.pool, REPO_LABEL, "find_by_ids", async |conn| {
            clusters::table
                .filter(clusters::id.eq_any(ids))
                .select(Cluster::as_select())
                .load(conn)
                .await
        })
        .await
    }

    pub async fn find_active(&self) -> RepoResult<Vec<Cluster>> {
        query(&self.pool, REPO_LABEL, "find_active", async |conn| {
            clusters::table
                .filter(clusters::confirmed_at.is_null())
                .filter(clusters::txids.ne(Vec::<String>::new()))
                .select(Cluster::as_select())
                .load(conn)
                .await
        })
        .await
    }

    pub async fn count(&self) -> RepoResult<i64> {
        query(&self.pool, REPO_LABEL, "count", async |conn| {
            clusters::table.count().get_result(conn).await
        })
        .await
    }
}
