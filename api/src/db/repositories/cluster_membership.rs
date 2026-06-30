use super::RepoResult;
use crate::db::models::{Cluster, NewCluster};
use crate::db::pool::DbPool;
use crate::db::schema::{clusters, transactions};
use diesel::prelude::*;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, RunQueryDsl};

pub struct ClusterMembershipUpdate<'a> {
    pub cluster_id: i64,
    pub current_members: &'a [String],
    pub total_vsize: i64,
    pub total_fee: i64,
}

#[derive(Clone)]
pub struct ClusterMembershipRepository {
    pool: DbPool,
}

impl ClusterMembershipRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Inserts a brand-new cluster and links its member txs
    pub async fn insert_with_members(&self, new: &NewCluster) -> RepoResult<Cluster> {
        let mut conn = self.pool.get().await?;
        let cluster = conn
            .transaction::<_, diesel::result::Error, _>(|conn| {
                async move {
                    let cluster = diesel::insert_into(clusters::table)
                        .values(new)
                        .returning(Cluster::as_returning())
                        .get_result(conn)
                        .await?;

                    diesel::update(transactions::table)
                        .filter(transactions::txid.eq_any(&new.txids))
                        .set(transactions::cluster_id.eq(cluster.id))
                        .execute(conn)
                        .await?;

                    Ok(cluster)
                }
                .scope_boxed()
            })
            .await?;
        Ok(cluster)
    }

    /// Updates the cluster fields and txid list, detaches any tx still linked to
    /// the cluster that is no longer a member, and (re)links the current members
    pub async fn replace_members(
        &self,
        update: ClusterMembershipUpdate<'_>,
    ) -> RepoResult<Cluster> {
        let ClusterMembershipUpdate {
            cluster_id,
            current_members: members,
            total_vsize,
            total_fee,
        } = update;
        let mut conn = self.pool.get().await?;
        let cluster = conn
            .transaction::<_, diesel::result::Error, _>(|conn| {
                async move {
                    // update the cluster with the new txid list and totals
                    let cluster = diesel::update(clusters::table.find(cluster_id))
                        .set((
                            clusters::txids.eq(members),
                            clusters::total_vsize.eq(total_vsize),
                            clusters::total_fee.eq(total_fee),
                        ))
                        .returning(Cluster::as_returning())
                        .get_result(conn)
                        .await?;

                    // detach txs that are no longer members of the cluster
                    diesel::update(transactions::table)
                        .filter(transactions::cluster_id.eq(cluster_id))
                        .filter(transactions::txid.ne_all(members))
                        .set(transactions::cluster_id.eq(None::<i64>))
                        .execute(conn)
                        .await?;

                    // attach current members to the cluster
                    diesel::update(transactions::table)
                        .filter(transactions::txid.eq_any(members))
                        .set(transactions::cluster_id.eq(cluster_id))
                        .execute(conn)
                        .await?;

                    Ok(cluster)
                }
                .scope_boxed()
            })
            .await?;
        Ok(cluster)
    }
}
