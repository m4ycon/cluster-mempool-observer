use crate::db::models::{Cluster, NewCluster};
use crate::db::{ClusterMembershipRepository, ClusterMembershipUpdate, ClusterRepository};
use crate::services::cluster_delta::ClusterDeltaService;
use futures::Stream;
use observer::error::ObserverError;
use observer::retrievers::{ClusterRetriever, ClusterRpcRetriever};
use shared::events::{ClusterDeltaEvent, ClusterRef};
use shared::metrics::timed_async_with;
use shared::models::GetMempoolClusterModel;
use std::collections::{HashMap, HashSet, VecDeque};
use time::OffsetDateTime;

/// Stages of one cluster resync round.
const CLUSTER_SYNC_SECONDS: &str = "cluster_sync_seconds";

/// Time to reconcile clusters against a newly mined block.
const CLUSTER_CONFIRM_MINED_SECONDS: &str = "cluster_confirm_mined_seconds";

#[derive(Clone)]
pub struct ClusterService<CR: ClusterRetriever = ClusterRpcRetriever> {
    cluster_repository: ClusterRepository,
    cluster_membership_repository: ClusterMembershipRepository,
    cluster_retriever: CR,
    cluster_delta_service: ClusterDeltaService,
}

impl<CR: ClusterRetriever> ClusterService<CR> {
    pub fn new(
        cluster_repository: ClusterRepository,
        cluster_membership_repository: ClusterMembershipRepository,
        cluster_retriever: CR,
        cluster_delta_service: ClusterDeltaService,
    ) -> Self {
        Self {
            cluster_repository,
            cluster_membership_repository,
            cluster_retriever,
            cluster_delta_service,
        }
    }

    pub async fn get_delta_stream(&self) -> impl Stream<Item = ClusterDeltaEvent> + use<CR> {
        self.cluster_delta_service.stream().await
    }

    pub fn get_current_snapshot(&self) -> Vec<ClusterDeltaEvent> {
        self.cluster_delta_service.get_current_snapshot()
    }

    pub fn active_cluster_count(&self) -> usize {
        self.cluster_delta_service.active_count()
    }

    pub async fn seed_snapshot(&self) {
        match self.cluster_repository.find_active().await {
            Ok(rows) => self
                .cluster_delta_service
                .seed(rows.into_iter().map(|c| ClusterRef {
                    id: c.id,
                    txids: c.txids,
                    total_vsize: c.total_vsize,
                    total_fee: c.total_fee,
                    first_seen_at: c.first_seen_at,
                })),
            Err(e) => tracing::error!("failed to seed cluster snapshot: {e}"),
        }
    }

    pub async fn sync_clusters_for(&self, candidate_txids: &[String], evicted_txids: &[String]) {
        let mut changes = ClusterDeltaSet::default();

        if !evicted_txids.is_empty() {
            changes.merge(
                timed_async_with(
                    CLUSTER_SYNC_SECONDS,
                    &[("stage", "evicted")],
                    self.handle_evicted(evicted_txids),
                )
                .await,
            );
        }

        if !candidate_txids.is_empty() {
            changes.merge(
                timed_async_with(
                    CLUSTER_SYNC_SECONDS,
                    &[("stage", "candidates")],
                    self.handle_candidates(candidate_txids),
                )
                .await,
            );
        }

        if changes.is_empty() {
            return;
        }

        timed_async_with(
            CLUSTER_SYNC_SECONDS,
            &[("stage", "publish")],
            self.publish_delta(changes),
        )
        .await;
    }

    /// Fetches and persists the clusters that the given candidate txids belong to.
    ///
    /// Runs as a work queue instead of a plain loop: an upsert can detach txs
    /// that the node no longer groups here, and every unconfirmed tx must end up
    /// in a cluster of its own. Detached txs go back into the queue so the node
    /// tells us where they belong now. This terminates because the cluster the
    /// node reports for a detached tx cannot detach anyone else - the tx already
    /// left its previous group.
    async fn handle_candidates(&self, candidate_txids: &[String]) -> ClusterDeltaSet {
        self.handle_candidates_covering(candidate_txids, HashSet::new())
            .await
    }

    /// Same as [`Self::handle_candidates`], but seeded with txids already known
    /// to be out of the mempool, so the queue never spends an RPC call asking
    /// the node about them.
    async fn handle_candidates_covering(
        &self,
        candidate_txids: &[String],
        mut out_of_mempool: HashSet<String>,
    ) -> ClusterDeltaSet {
        let mut changes = ClusterDeltaSet::default();
        let mut pending: VecDeque<String> = candidate_txids.iter().cloned().collect();
        // goal of this map is to batch insert the txids, less expensive, it's expected to be many singletons
        let mut fresh_singletons: HashMap<String, NewCluster> = HashMap::new();

        while let Some(txid) = pending.pop_front() {
            if out_of_mempool.contains(&txid) {
                continue;
            }

            let cluster = match self.cluster_retriever.get_mempool_cluster(&txid).await {
                Ok(cluster) => cluster,
                Err(ObserverError::TxNotFoundInMempool(_)) => {
                    out_of_mempool.insert(txid);
                    continue;
                }
                Err(e) => {
                    tracing::warn!("failed to fetch mempool cluster for {txid}: {e:?}");
                    continue;
                }
            };
            if cluster.txids.is_empty() {
                out_of_mempool.insert(txid);
                continue;
            }
            out_of_mempool.extend(cluster.txids.iter().cloned());

            let existing_ids = match self
                .cluster_repository
                .find_active_ids_by_txids(&cluster.txids)
                .await
            {
                Ok(ids) => ids,
                Err(e) => {
                    tracing::error!("failed to look up existing clusters: {e}");
                    continue;
                }
            };

            if existing_ids.is_empty() && cluster.txids.len() == 1 {
                fresh_singletons.insert(cluster.txids[0].clone(), new_cluster(&cluster));
                continue;
            }

            // the node grew a group past a buffered singleton, ensure we don't try to insert it as a new cluster
            for txid in &cluster.txids {
                fresh_singletons.remove(txid);
            }

            let outcome = self.upsert(cluster, existing_ids).await;
            changes.merge(outcome.changes);
            pending.extend(
                outcome
                    .detached
                    .into_iter()
                    .filter(|txid| !out_of_mempool.contains(txid)),
            );
        }

        // batch-insert the singletons that never merged with anything else, to avoid opening a transaction per cluster
        if !fresh_singletons.is_empty() {
            let batch: Vec<NewCluster> = fresh_singletons.into_values().collect();
            match self
                .cluster_membership_repository
                .insert_many_with_members(&batch)
                .await
            {
                Ok(rows) => {
                    for row in &rows {
                        changes.mark_upserted(row);
                    }
                }
                Err(e) => tracing::error!("failed to insert {} new clusters: {e}", batch.len()),
            }
        }

        changes
    }

    pub async fn confirm_mined(
        &self,
        txids: &[String],
        fees: &HashMap<String, i64>,
        sizes: &HashMap<String, i64>,
        confirmed_at: OffsetDateTime,
    ) {
        let changes = timed_async_with(
            CLUSTER_CONFIRM_MINED_SECONDS,
            &[("stage", "reconcile")],
            self.confirm_mined_inner(txids, fees, sizes, confirmed_at),
        )
        .await;
        timed_async_with(
            CLUSTER_CONFIRM_MINED_SECONDS,
            &[("stage", "publish")],
            self.publish_delta(changes),
        )
        .await;
    }

    async fn confirm_mined_inner(
        &self,
        txids: &[String],
        fees: &HashMap<String, i64>,
        sizes: &HashMap<String, i64>,
        confirmed_at: OffsetDateTime,
    ) -> ClusterDeltaSet {
        let mut changes = ClusterDeltaSet::default();
        let block_txids: HashSet<String> = txids.iter().cloned().collect();
        let cluster_ids = match self
            .cluster_repository
            .find_active_ids_by_txids(txids)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("failed to look up clusters for mined txs: {e}");
                return changes;
            }
        };
        if cluster_ids.is_empty() {
            return changes;
        }

        let clusters = match self.cluster_repository.find_by_ids(&cluster_ids).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("failed to load mined clusters: {e}");
                return changes;
            }
        };

        for cluster in clusters {
            let unconfirmed_txs: Vec<String> = cluster
                .txids
                .iter()
                .filter(|txid| !block_txids.contains(*txid))
                .cloned()
                .collect();

            // all txs cluster confirmed
            if unconfirmed_txs.is_empty() {
                match self
                    .cluster_membership_repository
                    .confirm(cluster.id, confirmed_at)
                    .await
                {
                    Ok(_) => changes.mark_removed(cluster.id),
                    Err(e) => tracing::error!("failed to confirm cluster {}: {e}", cluster.id),
                }
                continue;
            }

            // cluster partially confirmed

            let confirmed_txs: Vec<String> = cluster
                .txids
                .iter()
                .filter(|txid| block_txids.contains(*txid))
                .cloned()
                .collect();
            let total_fee: i64 = confirmed_txs.iter().filter_map(|txid| fees.get(txid)).sum();
            let total_vsize: i64 = confirmed_txs
                .iter()
                .filter_map(|txid| sizes.get(txid))
                .sum();

            if let Err(e) = self
                .cluster_membership_repository
                .replace_members(ClusterMembershipUpdate {
                    cluster_id: cluster.id,
                    current_members: &confirmed_txs,
                    total_vsize,
                    total_fee,
                })
                .await
            {
                tracing::error!(
                    "failed to keep confirmed txs on cluster {}: {e}",
                    cluster.id
                );
                continue;
            }
            match self
                .cluster_membership_repository
                .confirm(cluster.id, confirmed_at)
                .await
            {
                Ok(_) => changes.mark_removed(cluster.id),
                Err(e) => tracing::error!("failed to confirm cluster {}: {e}", cluster.id),
            }

            // let sync handle possible existing clusters for the still-pending txs
            let sync_changes = self.handle_candidates(&unconfirmed_txs).await;
            changes.merge(sync_changes);
        }
        changes
    }

    /// Closes the clusters that mempool eviction emptied out, and re-syncs the
    /// survivors of the ones that only shrank.
    async fn handle_evicted(&self, evicted_txids: &[String]) -> ClusterDeltaSet {
        let mut changes = ClusterDeltaSet::default();
        if evicted_txids.is_empty() {
            return changes;
        }

        let cluster_ids = match self
            .cluster_repository
            .find_active_ids_by_txids(evicted_txids)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("failed to look up clusters for evicted txs: {e}");
                return changes;
            }
        };
        if cluster_ids.is_empty() {
            return changes;
        }

        let clusters = match self.cluster_repository.find_by_ids(&cluster_ids).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("failed to load evicted clusters: {e}");
                return changes;
            }
        };

        let evicted: HashSet<String> = evicted_txids.iter().cloned().collect();
        let mut emptied: Vec<i64> = Vec::new();
        let mut survivors: Vec<String> = Vec::new();
        for cluster in clusters {
            if cluster.confirmed_at.is_some() || cluster.txids.is_empty() {
                continue;
            }
            let remaining: Vec<String> = cluster
                .txids
                .iter()
                .filter(|txid| !evicted.contains(*txid))
                .cloned()
                .collect();
            if remaining.len() == cluster.txids.len() {
                continue; // nothing evicted here
            }

            if remaining.is_empty() {
                emptied.push(cluster.id);
            } else {
                survivors.extend(remaining);
            }
        }

        if !emptied.is_empty() {
            match self
                .cluster_membership_repository
                .close_many(&emptied)
                .await
            {
                Ok(_) => {
                    for id in emptied {
                        changes.mark_removed(id);
                    }
                }
                Err(e) => tracing::error!("failed to close emptied clusters: {e}"),
            }
        }

        if !survivors.is_empty() {
            changes.merge(self.handle_candidates_covering(&survivors, evicted).await);
        }
        changes
    }

    /// Inserts a new cluster or updates the existing one(s) covering this group.
    ///
    /// `existing_ids` comes from the caller, which already looked it up to pick
    /// between this path and the batched insert.
    async fn upsert(
        &self,
        cluster: GetMempoolClusterModel,
        existing_ids: Vec<i64>,
    ) -> UpsertOutcome {
        let mut changes = ClusterDeltaSet::default();
        let total_fee = cluster.total_fee_sats as i64;
        let total_vsize = cluster.total_vsize();
        let mut detached: Vec<String> = Vec::new();
        let updated = if existing_ids.is_empty() {
            // no existing cluster, insert a new one
            match self
                .cluster_membership_repository
                .insert_with_members(&new_cluster(&cluster))
                .await
            {
                Ok(row) => row,
                Err(e) => {
                    tracing::error!("failed to insert cluster: {e}");
                    return UpsertOutcome::untouched(changes);
                }
            }
        } else {
            // existing cluster(s) exist, merge them into one and update it
            let existing = match self.cluster_repository.find_by_ids(&existing_ids).await {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!("failed to load existing clusters: {e}");
                    return UpsertOutcome::untouched(changes);
                }
            };
            let Some(keep) = existing.iter().min_by_key(|c| c.first_seen_at) else {
                return UpsertOutcome::untouched(changes);
            };

            // members of the old cluster(s) that the node no longer groups here:
            // they are about to be unlinked and need a cluster of their own
            let members: HashSet<&String> = cluster.txids.iter().collect();
            detached = existing
                .iter()
                .flat_map(|c| c.txids.iter())
                .filter(|txid| !members.contains(*txid))
                .cloned()
                .collect();

            let keep_id = keep.id;
            let clusters_to_remove: Vec<i64> = existing
                .iter()
                .map(|c| c.id)
                .filter(|id| *id != keep_id)
                .collect();
            if !clusters_to_remove.is_empty() {
                if let Err(e) = self
                    .cluster_membership_repository
                    .close_many(&clusters_to_remove)
                    .await
                {
                    tracing::error!("failed to close merged clusters: {e}");
                    return UpsertOutcome::untouched(changes);
                }
                for removed in &clusters_to_remove {
                    changes.mark_removed(*removed);
                }
            }

            match self
                .cluster_membership_repository
                .replace_members(ClusterMembershipUpdate {
                    cluster_id: keep_id,
                    current_members: &cluster.txids,
                    total_vsize,
                    total_fee,
                })
                .await
            {
                Ok(row) => row,
                Err(e) => {
                    tracing::error!("failed to update cluster: {e}");
                    // close_many may already have unlinked txs; re-queue them
                    return UpsertOutcome { changes, detached };
                }
            }
        };

        changes.mark_upserted(&updated);
        UpsertOutcome { changes, detached }
    }

    /// Hands the post-mutation state of the touched clusters, plus the removed
    /// ids, to the change service to diff against the snapshot and publish.
    async fn publish_delta(&self, changes: ClusterDeltaSet) {
        self.cluster_delta_service
            .publish(changes.upserted.into_values(), changes.removed)
            .await;
    }
}

fn new_cluster(cluster: &GetMempoolClusterModel) -> NewCluster {
    NewCluster {
        txids: cluster.txids.clone(),
        total_vsize: cluster.total_vsize(),
        total_fee: cluster.total_fee_sats as i64,
        first_seen_at: Some(OffsetDateTime::now_utc()),
    }
}

/// Result of one `upsert`: the clusters it touched, plus the txids it unlinked
/// from their previous cluster and that still need one.
struct UpsertOutcome {
    changes: ClusterDeltaSet,
    detached: Vec<String>,
}

impl UpsertOutcome {
    fn untouched(changes: ClusterDeltaSet) -> Self {
        Self {
            changes,
            detached: Vec::new(),
        }
    }
}

/// Clusters touched during one mutation round, handed to the cluster change service
/// to diff against the snapshot and emit a single `ClusterDeltaEvent`.
#[derive(Default)]
struct ClusterDeltaSet {
    upserted: HashMap<i64, ClusterRef>,
    removed: HashSet<i64>,
}

impl ClusterDeltaSet {
    fn mark_upserted(&mut self, cluster: &Cluster) {
        self.removed.remove(&cluster.id);
        self.upserted.insert(
            cluster.id,
            ClusterRef {
                id: cluster.id,
                txids: cluster.txids.clone(),
                total_vsize: cluster.total_vsize,
                total_fee: cluster.total_fee,
                first_seen_at: cluster.first_seen_at,
            },
        );
    }

    fn mark_removed(&mut self, id: i64) {
        self.upserted.remove(&id);
        self.removed.insert(id);
    }

    fn is_empty(&self) -> bool {
        self.upserted.is_empty() && self.removed.is_empty()
    }

    fn merge(&mut self, other: ClusterDeltaSet) {
        for (id, row) in other.upserted {
            self.removed.remove(&id);
            self.upserted.insert(id, row);
        }
        for id in other.removed {
            self.mark_removed(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cluster, ClusterDeltaSet};

    fn sorted(set: impl IntoIterator<Item = i64>) -> Vec<i64> {
        let mut v: Vec<i64> = set.into_iter().collect();
        v.sort();
        v
    }

    fn cluster(id: i64) -> Cluster {
        Cluster {
            id,
            txids: Vec::new(),
            total_vsize: 0,
            total_fee: 0,
            first_seen_at: None,
            confirmed_at: None,
        }
    }

    #[test]
    fn marking_upserted_then_removed_keeps_only_removed() {
        let mut changes = ClusterDeltaSet::default();
        changes.mark_upserted(&cluster(1));
        changes.mark_removed(1);
        assert!(changes.upserted.is_empty());
        assert_eq!(sorted(changes.removed), vec![1]);
    }

    #[test]
    fn marking_removed_then_upserted_keeps_only_upserted() {
        let mut changes = ClusterDeltaSet::default();
        changes.mark_removed(1);
        changes.mark_upserted(&cluster(1));
        assert!(changes.removed.is_empty());
        assert_eq!(sorted(changes.upserted.into_keys()), vec![1]);
    }

    #[test]
    fn merge_unions_disjoint_ids() {
        let mut base = ClusterDeltaSet::default();
        base.mark_upserted(&cluster(1));
        base.mark_removed(2);

        let mut other = ClusterDeltaSet::default();
        other.mark_upserted(&cluster(3));
        other.mark_removed(4);

        base.merge(other);
        assert_eq!(sorted(base.upserted.into_keys()), vec![1, 3]);
        assert_eq!(sorted(base.removed), vec![2, 4]);
    }

    #[test]
    fn merge_lets_other_win_on_conflicts() {
        let mut base = ClusterDeltaSet::default();
        base.mark_upserted(&cluster(1)); // base upserts 1
        base.mark_removed(2); // base removes 2

        let mut other = ClusterDeltaSet::default();
        other.mark_removed(1); // other removed it later
        other.mark_upserted(&cluster(2)); // other brought it back later

        base.merge(other);
        assert_eq!(sorted(base.upserted.into_keys()), vec![2]);
        assert_eq!(sorted(base.removed), vec![1]);
    }
}
