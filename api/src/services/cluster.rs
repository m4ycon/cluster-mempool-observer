use crate::db::models::NewCluster;
use crate::db::{ClusterRepository, TransactionRepository};
use crate::services::cluster_delta::ClusterDeltaService;
use futures::Stream;
use observer::retrievers::{ClusterRetriever, ClusterRpcRetriever};
use shared::events::ClusterDeltaEvent;
use shared::models::GetMempoolClusterModel;
use std::collections::{HashMap, HashSet};
use time::OffsetDateTime;

#[derive(Clone)]
pub struct ClusterService<CR: ClusterRetriever = ClusterRpcRetriever> {
    cluster_repository: ClusterRepository,
    transaction_repository: TransactionRepository,
    cluster_retriever: CR,
    cluster_delta_service: ClusterDeltaService,
}

impl<CR: ClusterRetriever> ClusterService<CR> {
    pub fn new(
        cluster_repository: ClusterRepository,
        transaction_repository: TransactionRepository,
        cluster_retriever: CR,
        cluster_delta_service: ClusterDeltaService,
    ) -> Self {
        Self {
            cluster_repository,
            transaction_repository,
            cluster_retriever,
            cluster_delta_service,
        }
    }

    pub async fn get_delta_stream(&self) -> impl Stream<Item = ClusterDeltaEvent> + use<CR> {
        self.cluster_delta_service.stream().await
    }

    pub fn get_current_snapshot(&self) -> ClusterDeltaEvent {
        self.cluster_delta_service.get_current_snapshot()
    }

    pub async fn seed_snapshot(&self) {
        match self.cluster_repository.find_active().await {
            Ok(rows) => self
                .cluster_delta_service
                .seed(
                    rows.into_iter()
                        .map(|c| (c.id, c.txids, c.total_size, c.total_fee)),
                ),
            Err(e) => tracing::error!("failed to seed cluster snapshot: {e}"),
        }
    }

    pub async fn sync_clusters_for(&self, candidate_txids: &[String]) {
        let changes = self.sync_clusters_for_inner(candidate_txids).await;
        self.publish_delta(changes).await;
    }

    /// Fetches and persists the clusters that the given candidate txids belong to.
    async fn sync_clusters_for_inner(&self, candidate_txids: &[String]) -> ClusterDeltaSet {
        let mut changes = ClusterDeltaSet::default();
        let mut covered: HashSet<String> = HashSet::new();
        for txid in candidate_txids {
            if covered.contains(txid) {
                continue;
            }

            let cluster = match self.cluster_retriever.get_mempool_cluster(txid).await {
                Ok(cluster) => cluster,
                Err(e) => {
                    tracing::warn!("failed to fetch mempool cluster for {txid}: {e:?}");
                    continue;
                }
            };
            covered.extend(cluster.txids.iter().cloned());

            if cluster.tx_count <= 1 {
                continue;
            }

            let upsert_changes = self.upsert(cluster).await;
            changes.merge(upsert_changes);
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
        let changes = self
            .confirm_mined_inner(txids, fees, sizes, confirmed_at)
            .await;
        self.publish_delta(changes).await;
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
            .transaction_repository
            .get_cluster_ids_by_txids(txids)
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
                    .cluster_repository
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
            let total_size: i64 = confirmed_txs.iter().filter_map(|txid| sizes.get(txid)).sum();

            if let Err(e) = self
                .cluster_repository
                .update(cluster.id, &confirmed_txs, total_size, total_fee)
                .await
            {
                tracing::error!(
                    "failed to keep confirmed txs on cluster {}: {e}",
                    cluster.id
                );
                continue;
            }
            match self
                .cluster_repository
                .confirm(cluster.id, confirmed_at)
                .await
            {
                Ok(_) => changes.mark_removed(cluster.id),
                Err(e) => tracing::error!("failed to confirm cluster {}: {e}", cluster.id),
            }

            // detach the still-pending txs from this now-confirmed cluster
            if let Err(e) = self
                .transaction_repository
                .clear_cluster_id(&unconfirmed_txs)
                .await
            {
                tracing::error!(
                    "failed to detach unconfirmed txs from cluster {}: {e}",
                    cluster.id
                );
            }
            // let sync handle possible existing clusters for the still-pending txs
            let sync_changes = self.sync_clusters_for_inner(&unconfirmed_txs).await;
            changes.merge(sync_changes);
        }
        changes
    }

    /// Inserts a new cluster or updates the existing one(s) covering this group
    async fn upsert(&self, cluster: GetMempoolClusterModel) -> ClusterDeltaSet {
        let mut changes = ClusterDeltaSet::default();
        let existing_ids = match self
            .transaction_repository
            .get_cluster_ids_by_txids(&cluster.txids)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("failed to look up existing clusters: {e}");
                return changes;
            }
        };

        let total_fee = cluster.total_fee_sats as i64;
        let total_size = cluster.total_vsize();
        let id = if existing_ids.is_empty() {
            let new_cluster = NewCluster {
                txids: cluster.txids.clone(),
                total_size,
                total_fee,
                first_seen_at: Some(OffsetDateTime::now_utc()),
            };
            match self.cluster_repository.insert(&new_cluster).await {
                Ok(row) => row.id,
                Err(e) => {
                    tracing::error!("failed to insert cluster: {e}");
                    return changes;
                }
            }
        } else {
            let existing = match self.cluster_repository.find_by_ids(&existing_ids).await {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!("failed to load existing clusters: {e}");
                    return changes;
                }
            };
            let Some(keep) = existing.iter().min_by_key(|c| c.first_seen_at) else {
                return changes;
            };

            let keep_id = keep.id;
            let clusters_to_delete: Vec<i64> = existing
                .iter()
                .map(|c| c.id)
                .filter(|id| *id != keep_id)
                .collect();
            if !clusters_to_delete.is_empty() {
                if let Err(e) = self
                    .cluster_repository
                    .delete_many(&clusters_to_delete)
                    .await
                {
                    tracing::error!("failed to delete merged clusters: {e}");
                    return changes;
                }
                for deleted in &clusters_to_delete {
                    changes.mark_removed(*deleted);
                }
            }

            match self
                .cluster_repository
                .update(keep_id, &cluster.txids, total_size, total_fee)
                .await
            {
                Ok(row) => row.id,
                Err(e) => {
                    tracing::error!("failed to update cluster: {e}");
                    return changes;
                }
            }
        };

        changes.mark_upserted(id);

        if let Err(e) = self
            .transaction_repository
            .set_cluster_id(&cluster.txids, id)
            .await
        {
            tracing::error!("failed to link transactions to cluster {id}: {e}");
        }

        changes
    }

    /// Loads the current state of the touched clusters and hands them, plus the
    /// removed ids, to the change service to diff and publish.
    async fn publish_delta(&self, changes: ClusterDeltaSet) {
        let upserted = if changes.upserted.is_empty() {
            Vec::new()
        } else {
            let ids: Vec<i64> = changes.upserted.into_iter().collect();
            match self.cluster_repository.find_by_ids(&ids).await {
                Ok(rows) => rows
                    .into_iter()
                    .map(|row| (row.id, row.txids, row.total_size, row.total_fee))
                    .collect(),
                Err(e) => {
                    tracing::error!("failed to load upserted clusters for change event: {e}");
                    Vec::new()
                }
            }
        };

        self.cluster_delta_service
            .publish(upserted, changes.removed)
            .await;
    }
}

/// Cluster ids touched during one mutation round, handed to the cluster change service
/// to diff against the snapshot and emit a single `ClusterDeltaEvent`.
#[derive(Default)]
struct ClusterDeltaSet {
    upserted: HashSet<i64>,
    removed: HashSet<i64>,
}

impl ClusterDeltaSet {
    fn mark_upserted(&mut self, id: i64) {
        self.removed.remove(&id);
        self.upserted.insert(id);
    }

    fn mark_removed(&mut self, id: i64) {
        self.upserted.remove(&id);
        self.removed.insert(id);
    }

    fn merge(&mut self, other: ClusterDeltaSet) {
        for id in other.upserted {
            self.mark_upserted(id);
        }
        for id in other.removed {
            self.mark_removed(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ClusterDeltaSet;

    fn sorted(set: impl IntoIterator<Item = i64>) -> Vec<i64> {
        let mut v: Vec<i64> = set.into_iter().collect();
        v.sort();
        v
    }

    #[test]
    fn marking_upserted_then_removed_keeps_only_removed() {
        let mut changes = ClusterDeltaSet::default();
        changes.mark_upserted(1);
        changes.mark_removed(1);
        assert!(changes.upserted.is_empty());
        assert_eq!(sorted(changes.removed), vec![1]);
    }

    #[test]
    fn marking_removed_then_upserted_keeps_only_upserted() {
        let mut changes = ClusterDeltaSet::default();
        changes.mark_removed(1);
        changes.mark_upserted(1);
        assert!(changes.removed.is_empty());
        assert_eq!(sorted(changes.upserted), vec![1]);
    }

    #[test]
    fn merge_unions_disjoint_ids() {
        let mut base = ClusterDeltaSet::default();
        base.mark_upserted(1);
        base.mark_removed(2);

        let mut other = ClusterDeltaSet::default();
        other.mark_upserted(3);
        other.mark_removed(4);

        base.merge(other);
        assert_eq!(sorted(base.upserted), vec![1, 3]);
        assert_eq!(sorted(base.removed), vec![2, 4]);
    }

    #[test]
    fn merge_lets_other_win_on_conflicts() {
        let mut base = ClusterDeltaSet::default();
        base.mark_upserted(1); // base upserts 1
        base.mark_removed(2); // base removes 2

        let mut other = ClusterDeltaSet::default();
        other.mark_removed(1); // other removed it later
        other.mark_upserted(2); // other brought it back later

        base.merge(other);
        assert_eq!(sorted(base.upserted), vec![2]);
        assert_eq!(sorted(base.removed), vec![1]);
    }
}
