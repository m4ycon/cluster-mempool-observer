use crate::db::models::NewCluster;
use crate::db::{ClusterRepository, TransactionRepository};
use observer::retrievers::{ClusterRetriever, ClusterRpcRetriever};
use shared::models::GetMempoolClusterModel;
use std::collections::{HashMap, HashSet};
use time::OffsetDateTime;

#[derive(Clone)]
pub struct ClusterService<CR: ClusterRetriever = ClusterRpcRetriever> {
    cluster_repository: ClusterRepository,
    transaction_repository: TransactionRepository,
    cluster_retriever: CR,
}

impl<CR: ClusterRetriever> ClusterService<CR> {
    pub fn new(
        cluster_repository: ClusterRepository,
        transaction_repository: TransactionRepository,
        cluster_retriever: CR,
    ) -> Self {
        Self {
            cluster_repository,
            transaction_repository,
            cluster_retriever,
        }
    }

    /// Fetches and persists the clusters that the given candidate txids belong to
    pub async fn sync_clusters_for(&self, candidate_txids: &[String]) {
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

            self.upsert(cluster).await;
        }
    }

    pub async fn confirm_mined(
        &self,
        txids: &[String],
        fees: &HashMap<String, i64>,
        confirmed_at: OffsetDateTime,
    ) {
        let block_txids: HashSet<String> = txids.iter().cloned().collect();
        let cluster_ids = match self
            .transaction_repository
            .get_cluster_ids_by_txids(txids)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("failed to look up clusters for mined txs: {e}");
                return;
            }
        };
        if cluster_ids.is_empty() {
            return;
        }

        let clusters = match self.cluster_repository.find_by_ids(&cluster_ids).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("failed to load mined clusters: {e}");
                return;
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
                if let Err(e) = self
                    .cluster_repository
                    .confirm(cluster.id, confirmed_at)
                    .await
                {
                    tracing::error!("failed to confirm cluster {}: {e}", cluster.id);
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

            if let Err(e) = self
                .cluster_repository
                .update(cluster.id, &confirmed_txs, total_fee)
                .await
            {
                tracing::error!(
                    "failed to keep confirmed txs on cluster {}: {e}",
                    cluster.id
                );
                continue;
            }
            if let Err(e) = self
                .cluster_repository
                .confirm(cluster.id, confirmed_at)
                .await
            {
                tracing::error!("failed to confirm cluster {}: {e}", cluster.id);
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
            self.sync_clusters_for(&unconfirmed_txs).await;
        }
    }

    /// Inserts a new cluster or updates the existing one(s) covering this group
    async fn upsert(&self, cluster: GetMempoolClusterModel) {
        let existing_ids = match self
            .transaction_repository
            .get_cluster_ids_by_txids(&cluster.txids)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("failed to look up existing clusters: {e}");
                return;
            }
        };

        let total_fee = cluster.total_fee_sats as i64;
        let id = if existing_ids.is_empty() {
            let new_cluster = NewCluster {
                txids: cluster.txids.clone(),
                total_fee,
                first_seen_at: Some(OffsetDateTime::now_utc()),
            };
            match self.cluster_repository.insert(&new_cluster).await {
                Ok(row) => row.id,
                Err(e) => {
                    tracing::error!("failed to insert cluster: {e}");
                    return;
                }
            }
        } else {
            let existing = match self.cluster_repository.find_by_ids(&existing_ids).await {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!("failed to load existing clusters: {e}");
                    return;
                }
            };
            let Some(keep) = existing.iter().min_by_key(|c| c.first_seen_at) else {
                return;
            };

            let keep_id = keep.id;
            let clusters_to_delete: Vec<i64> = existing
                .iter()
                .map(|c| c.id)
                .filter(|id| *id != keep_id)
                .collect();
            if !clusters_to_delete.is_empty()
                && let Err(e) = self
                    .cluster_repository
                    .delete_many(&clusters_to_delete)
                    .await
            {
                tracing::error!("failed to delete merged clusters: {e}");
                return;
            }

            match self
                .cluster_repository
                .update(keep_id, &cluster.txids, total_fee)
                .await
            {
                Ok(row) => row.id,
                Err(e) => {
                    tracing::error!("failed to update cluster: {e}");
                    return;
                }
            }
        };

        if let Err(e) = self
            .transaction_repository
            .set_cluster_id(&cluster.txids, id)
            .await
        {
            tracing::error!("failed to link transactions to cluster {id}: {e}");
        }
    }
}
