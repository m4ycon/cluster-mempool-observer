use observer::error::ObserverError;
use observer::retrievers::{ClusterRetriever, TransactionRetriever};
use shared::models::{GetMempoolClusterModel, GetRawTransactionModel};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct MockTransactionRetriever {
    txs_fetched: Arc<Mutex<Vec<String>>>,
    fail_for: Arc<Vec<String>>,
}

impl MockTransactionRetriever {
    /// Builds a retriever that returns an error for the given txids
    pub fn failing_for(txids: impl IntoIterator<Item = String>) -> Self {
        Self {
            fail_for: Arc::new(txids.into_iter().collect()),
            ..Self::default()
        }
    }

    pub fn txs_fetched(&self) -> Vec<String> {
        let mut ids = self.txs_fetched.lock().unwrap().clone();
        ids.sort();
        ids
    }
}

impl TransactionRetriever for MockTransactionRetriever {
    async fn get_raw_transaction(
        &self,
        txid: &str,
    ) -> Result<GetRawTransactionModel, ObserverError> {
        self.txs_fetched.lock().unwrap().push(txid.to_string());
        if self.fail_for.iter().any(|t| t == txid) {
            return Err(ObserverError::FailedToFetch(txid.to_string()));
        }
        Ok(GetRawTransactionModel {
            txid: txid.to_string(),
            version: 0,
            lock_time: 0,
            vsize: 0,
            weight: 0,
            input_count: 0,
            input_txids: vec![],
            output_count: 0,
            confirmations: 0,
            time: None,
        })
    }
}

#[derive(Clone, Default)]
pub struct MockClusterRetriever {
    clusters_fetched: Arc<Mutex<Vec<String>>>,
    seed_clusters: Arc<HashMap<String, GetMempoolClusterModel>>,
}

impl MockClusterRetriever {
    /// Builds a retriever that returns the given cluster for each of its member txids
    pub fn with_clusters(clusters: Vec<GetMempoolClusterModel>) -> Self {
        let mut seed = HashMap::new();
        for cluster in clusters {
            for txid in &cluster.txids {
                seed.insert(txid.clone(), cluster.clone());
            }
        }
        Self {
            seed_clusters: Arc::new(seed),
            ..Self::default()
        }
    }

    pub fn clusters_fetched(&self) -> Vec<String> {
        let mut ids = self.clusters_fetched.lock().unwrap().clone();
        ids.sort();
        ids
    }
}

impl ClusterRetriever for MockClusterRetriever {
    async fn get_mempool_cluster(
        &self,
        txid: &str,
    ) -> Result<GetMempoolClusterModel, ObserverError> {
        self.clusters_fetched.lock().unwrap().push(txid.to_string());
        match self.seed_clusters.get(txid) {
            Some(cluster) => Ok(cluster.clone()),
            None => Ok(GetMempoolClusterModel {
                cluster_weight: 0,
                tx_count: 1,
                txids: vec![txid.to_string()],
                total_fee_sats: 0,
            }),
        }
    }
}
