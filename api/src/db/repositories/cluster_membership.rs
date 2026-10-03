use super::{MEMPOOL_DELTA_INSERT_CHUNK_SIZE, RepoResult, TRANSACTION_INSERT_CHUNK_SIZE};
use crate::db::instrument::query;
use crate::db::models::{Cluster, ClusterStatus, NewCluster, NewClusterDelta, NewTransaction};
use crate::db::pool::DbPool;
use crate::db::schema::{cluster_deltas, clusters, transactions};
use diesel::prelude::*;
use diesel::sql_types::{Array, BigInt};
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

/// Postgres caps a statement at 65535 bind params; `NewClusterDelta` has 5 columns.
const CLUSTER_DELTA_INSERT_CHUNK_SIZE: usize = 10_000;

/// The two ways a cluster's active life can end without being mined.
#[derive(Clone, Copy)]
enum ClusterClosing {
    /// Every member of the cluster has left the mempool without being confirmed.
    Evicted,
    /// This cluster has been merged into another cluster.
    Merged,
}

impl From<ClusterClosing> for ClusterStatus {
    fn from(closing: ClusterClosing) -> Self {
        match closing {
            ClusterClosing::Evicted => ClusterStatus::Evicted,
            ClusterClosing::Merged => ClusterStatus::Merged,
        }
    }
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
        query(
            &self.pool,
            REPO_LABEL,
            "insert_with_members",
            async |conn| {
                conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                    let cluster = diesel::insert_into(clusters::table)
                        .values(new)
                        .returning(Cluster::as_returning())
                        .get_result(conn)
                        .await?;

                    Self::insert_hollow_members(conn, &new.txids).await?;

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
                for chunk in new.chunks(MEMPOOL_DELTA_INSERT_CHUNK_SIZE) {
                    let rows = conn
                        .transaction::<_, diesel::result::Error, _>(async |conn| {
                            let rows: Vec<Cluster> = diesel::insert_into(clusters::table)
                                .values(chunk)
                                .returning(Cluster::as_returning())
                                .get_results(conn)
                                .await?;

                            let members: Vec<String> =
                                rows.iter().flat_map(|c| c.txids.iter().cloned()).collect();
                            Self::insert_hollow_members(conn, &members).await?;

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
        query(&self.pool, REPO_LABEL, "replace_members", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                Self::replace_members_in(conn, &update).await
            })
            .await
        })
        .await
    }

    async fn replace_members_in(
        conn: &mut AsyncPgConnection,
        update: &ClusterMembershipUpdate<'_>,
    ) -> Result<Cluster, diesel::result::Error> {
        let &ClusterMembershipUpdate {
            cluster_id,
            current_members: members,
            total_vsize,
            total_fee,
        } = update;

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

        Self::insert_hollow_members(conn, members).await?;

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

    pub async fn mark_evicted(&self, ids: &[i64]) -> RepoResult<usize> {
        self.close_many(ids, ClusterClosing::Evicted).await
    }

    pub async fn mark_merged(&self, ids: &[i64]) -> RepoResult<usize> {
        self.close_many(ids, ClusterClosing::Merged).await
    }

    /// Ends the active life of clusters that merged away or lost their members.
    async fn close_many(&self, ids: &[i64], closing: ClusterClosing) -> RepoResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let status = ClusterStatus::from(closing);

        query(&self.pool, REPO_LABEL, "close_many", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                let rows: Vec<Cluster> = clusters::table
                    .filter(clusters::id.eq_any(ids))
                    .filter(clusters::status.eq(ClusterStatus::Active))
                    .select(Cluster::as_select())
                    .for_update()
                    .load(conn)
                    .await?;

                for cluster in &rows {
                    diesel::update(clusters::table.find(cluster.id))
                        .set(clusters::status.eq(status))
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
            })
            .await
        })
        .await
    }

    /// Marks clusters as confirmed and logs the closing delta row that ends
    /// each one's membership in log-space, in one transaction: all of them or
    /// none. The rows keep their txids and totals so confirmed member txs stay
    /// linked. Re-confirming is a no-op. Returns how many were newly confirmed.
    pub async fn confirm_many(
        &self,
        ids: &[i64],
        confirmed_at: OffsetDateTime,
    ) -> RepoResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        query(&self.pool, REPO_LABEL, "confirm_many", async |conn| {
            conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                Self::confirm_many_in(conn, ids, confirmed_at).await
            })
            .await
        })
        .await
    }

    async fn confirm_many_in(
        conn: &mut AsyncPgConnection,
        ids: &[i64],
        confirmed_at: OffsetDateTime,
    ) -> Result<usize, diesel::result::Error> {
        // id order, so two writers locking overlapping sets cannot deadlock
        let rows: Vec<Cluster> = clusters::table
            .filter(clusters::id.eq_any(ids))
            .filter(clusters::confirmed_at.is_null())
            .order(clusters::id.asc())
            .select(Cluster::as_select())
            .for_update()
            .load(conn)
            .await?;
        if rows.is_empty() {
            return Ok(0);
        }

        let pending: Vec<i64> = rows.iter().map(|c| c.id).collect();
        diesel::update(clusters::table.filter(clusters::id.eq_any(&pending)))
            .set((
                clusters::confirmed_at.eq(confirmed_at),
                clusters::status.eq(ClusterStatus::Confirmed),
            ))
            .execute(conn)
            .await?;

        let deltas: Vec<NewClusterDelta> = rows
            .iter()
            .filter(|c| !c.txids.is_empty())
            .map(|c| NewClusterDelta {
                cluster_id: c.id,
                added_txids: Vec::new(),
                removed_txids: c.txids.clone(),
                fee_delta: -c.total_fee,
                vsize_delta: -c.total_vsize,
            })
            .collect();
        for chunk in deltas.chunks(CLUSTER_DELTA_INSERT_CHUNK_SIZE) {
            diesel::insert_into(cluster_deltas::table)
                .values(chunk)
                .execute(conn)
                .await?;
        }

        Ok(rows.len())
    }

    /// [`Self::replace_members`] on each cluster, then [`Self::confirm_many`]
    /// on all of them, in one transaction: all of them or none. Returns how
    /// many were newly confirmed.
    pub async fn trim_and_confirm_many(
        &self,
        updates: &[ClusterMembershipUpdate<'_>],
        confirmed_at: OffsetDateTime,
    ) -> RepoResult<usize> {
        if updates.is_empty() {
            return Ok(0);
        }
        // id order, so two writers locking overlapping sets cannot deadlock
        let mut updates: Vec<&ClusterMembershipUpdate<'_>> = updates.iter().collect();
        updates.sort_by_key(|update| update.cluster_id);
        let ids: Vec<i64> = updates.iter().map(|update| update.cluster_id).collect();

        query(
            &self.pool,
            REPO_LABEL,
            "trim_and_confirm_many",
            async |conn| {
                conn.transaction::<_, diesel::result::Error, _>(async |conn| {
                    for update in &updates {
                        Self::replace_members_in(conn, update).await?;
                    }
                    Self::confirm_many_in(conn, &ids, confirmed_at).await
                })
                .await
            },
        )
        .await
    }

    async fn insert_hollow_members(
        conn: &mut AsyncPgConnection,
        txids: &[String],
    ) -> Result<(), diesel::result::Error> {
        if txids.is_empty() {
            return Ok(());
        }

        let rows: Vec<NewTransaction> = txids
            .iter()
            .map(|txid| NewTransaction::hollow(txid))
            .collect();
        let rows = NewTransaction::sorted_by_txid(&rows);
        for chunk in rows.chunks(TRANSACTION_INSERT_CHUNK_SIZE) {
            diesel::insert_into(transactions::table)
                .values(chunk.to_vec())
                .on_conflict(transactions::txid)
                .do_nothing()
                .execute(conn)
                .await?;
        }
        Ok(())
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
