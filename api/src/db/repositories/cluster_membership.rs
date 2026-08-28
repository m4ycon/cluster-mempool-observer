use super::{INSERT_CHUNK_SIZE, RepoResult};
use crate::db::instrument::query;
use crate::db::models::{Cluster, NewCluster, NewClusterDelta};
use crate::db::pool::DbPool;
use crate::db::schema::{cluster_deltas, clusters, transactions};
use diesel::prelude::*;
use diesel::sql_types::{Array, BigInt};
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use std::collections::HashSet;
use time::OffsetDateTime;

pub struct ClusterMembershipUpdate<'a> {
    pub cluster_id: i64,
    pub current_members: &'a [String],
    pub total_vsize: i64,
    pub total_fee: i64,
}

const REPO_LABEL: &str = "cluster_membership";

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
        query(
            &self.pool,
            REPO_LABEL,
            "insert_with_members",
            async |conn| {
                conn.transaction::<_, diesel::result::Error, _>(|conn| {
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

                        Self::log_delta(
                            conn,
                            NewClusterDelta {
                                cluster_id: cluster.id,
                                added_txids: new.txids.clone(),
                                removed_txids: Vec::new(),
                                fee_delta: new.total_fee,
                                vsize_delta: new.total_vsize,
                            },
                        )
                        .await?;

                        Ok(cluster)
                    }
                    .scope_boxed()
                })
                .await
            },
        )
        .await
    }

    /// Inserts a batch of brand-new clusters and links their member txs, one
    /// database transaction per chunk instead of one per cluster.
    pub async fn insert_many_with_members(&self, new: &[NewCluster]) -> RepoResult<Vec<Cluster>> {
        if new.is_empty() {
            return Ok(Vec::new());
        }

        query(
            &self.pool,
            REPO_LABEL,
            "insert_many_with_members",
            async |conn| {
                let mut inserted: Vec<Cluster> = Vec::with_capacity(new.len());
                for chunk in new.chunks(INSERT_CHUNK_SIZE) {
                    let rows = conn
                        .transaction::<_, diesel::result::Error, _>(|conn| {
                            async move {
                                let rows: Vec<Cluster> = diesel::insert_into(clusters::table)
                                    .values(chunk)
                                    .returning(Cluster::as_returning())
                                    .get_results(conn)
                                    .await?;

                                // link the member txs in one statement
                                let ids: Vec<i64> = rows.iter().map(|c| c.id).collect();
                                diesel::sql_query(
                                    "UPDATE transactions t \
                                        SET cluster_id = c.id \
                                       FROM (SELECT id, unnest(txids) AS txid \
                                               FROM clusters WHERE id = ANY($1)) c \
                                      WHERE t.txid = c.txid",
                                )
                                .bind::<Array<BigInt>, _>(ids)
                                .execute(conn)
                                .await?;

                                let deltas: Vec<NewClusterDelta> = rows
                                    .iter()
                                    .map(|cluster| NewClusterDelta {
                                        cluster_id: cluster.id,
                                        added_txids: cluster.txids.clone(),
                                        removed_txids: Vec::new(),
                                        fee_delta: cluster.total_fee,
                                        vsize_delta: cluster.total_vsize,
                                    })
                                    .collect();
                                diesel::insert_into(cluster_deltas::table)
                                    .values(&deltas)
                                    .execute(conn)
                                    .await?;

                                Ok(rows)
                            }
                            .scope_boxed()
                        })
                        .await?;
                    inserted.extend(rows);
                }
                Ok(inserted)
            },
        )
        .await
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

        query(&self.pool, REPO_LABEL, "replace_members", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                async move {
                    let old: Cluster = clusters::table
                        .find(cluster_id)
                        .select(Cluster::as_select())
                        .for_update()
                        .first(conn)
                        .await?;

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

                    let old_set: HashSet<&String> = old.txids.iter().collect();
                    let new_set: HashSet<&String> = members.iter().collect();
                    let added_txids: Vec<String> = members
                        .iter()
                        .filter(|txid| !old_set.contains(*txid))
                        .cloned()
                        .collect();
                    let removed_txids: Vec<String> = old
                        .txids
                        .iter()
                        .filter(|txid| !new_set.contains(*txid))
                        .cloned()
                        .collect();
                    let fee_delta = total_fee - old.total_fee;
                    let vsize_delta = total_vsize - old.total_vsize;

                    // skip no-op rounds: upsert re-syncs unchanged clusters constantly
                    let is_noop = added_txids.is_empty()
                        && removed_txids.is_empty()
                        && fee_delta == 0
                        && vsize_delta == 0;
                    if !is_noop {
                        Self::log_delta(
                            conn,
                            NewClusterDelta {
                                cluster_id,
                                added_txids,
                                removed_txids,
                                fee_delta,
                                vsize_delta,
                            },
                        )
                        .await?;
                    }

                    Ok(cluster)
                }
                .scope_boxed()
            })
            .await
        })
        .await
    }

    /// Closes clusters that merged away or lost their members: empties the
    /// membership and totals, detaches the member txs, and logs a closing
    /// delta row. Rows are never deleted so the delta log stays FK-valid.
    pub async fn close_many(&self, ids: &[i64]) -> RepoResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        query(&self.pool, REPO_LABEL, "close_many", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                async move {
                    let rows: Vec<Cluster> = clusters::table
                        .filter(clusters::id.eq_any(ids))
                        .filter(clusters::txids.ne(Vec::<String>::new()))
                        .select(Cluster::as_select())
                        .for_update()
                        .load(conn)
                        .await?;

                    for cluster in &rows {
                        diesel::update(clusters::table.find(cluster.id))
                            .set((
                                clusters::txids.eq(Vec::<String>::new()),
                                clusters::total_fee.eq(0i64),
                                clusters::total_vsize.eq(0i64),
                            ))
                            .execute(conn)
                            .await?;

                        diesel::update(transactions::table)
                            .filter(transactions::cluster_id.eq(cluster.id))
                            .set(transactions::cluster_id.eq(None::<i64>))
                            .execute(conn)
                            .await?;

                        Self::log_delta(
                            conn,
                            NewClusterDelta {
                                cluster_id: cluster.id,
                                added_txids: Vec::new(),
                                removed_txids: cluster.txids.clone(),
                                fee_delta: -cluster.total_fee,
                                vsize_delta: -cluster.total_vsize,
                            },
                        )
                        .await?;
                    }

                    Ok(rows.len())
                }
                .scope_boxed()
            })
            .await
        })
        .await
    }

    /// Marks a cluster as confirmed and logs the closing delta row that ends
    /// its membership in log-space. The row keeps its txids and totals so
    /// confirmed member txs stay linked. Re-confirming is a no-op.
    pub async fn confirm(&self, id: i64, confirmed_at: OffsetDateTime) -> RepoResult<Cluster> {
        query(&self.pool, REPO_LABEL, "confirm", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                async move {
                    let old: Cluster = clusters::table
                        .find(id)
                        .select(Cluster::as_select())
                        .for_update()
                        .first(conn)
                        .await?;
                    if old.confirmed_at.is_some() {
                        return Ok(old);
                    }

                    let cluster = diesel::update(clusters::table.find(id))
                        .set(clusters::confirmed_at.eq(confirmed_at))
                        .returning(Cluster::as_returning())
                        .get_result(conn)
                        .await?;

                    if !old.txids.is_empty() {
                        Self::log_delta(
                            conn,
                            NewClusterDelta {
                                cluster_id: id,
                                added_txids: Vec::new(),
                                removed_txids: old.txids.clone(),
                                fee_delta: -old.total_fee,
                                vsize_delta: -old.total_vsize,
                            },
                        )
                        .await?;
                    }

                    Ok(cluster)
                }
                .scope_boxed()
            })
            .await
        })
        .await
    }

    /// Appends one row to the cluster_deltas event log. Must run inside the
    /// same transaction as the membership mutation it describes.
    async fn log_delta(
        conn: &mut AsyncPgConnection,
        row: NewClusterDelta,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(cluster_deltas::table)
            .values(&row)
            .execute(conn)
            .await?;
        Ok(())
    }
}
