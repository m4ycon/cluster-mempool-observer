use observer::error::ObserverError;
use observer::retrievers::{
    BlockRetriever, ClusterRetriever, NetworkRetriever, TransactionRetriever,
};
use shared::models::{
    GetBlockModel, GetMempoolClusterModel, GetNetworkInfoModel, GetRawTransactionModel,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct MockBlockRetriever {
    blocks: Arc<HashMap<String, GetBlockModel>>,
}

impl MockBlockRetriever {
    /// Builds a retriever that returns each block keyed by its hash
    pub fn with_blocks(blocks: Vec<GetBlockModel>) -> Self {
        let map = blocks.into_iter().map(|b| (b.hash.clone(), b)).collect();
        Self {
            blocks: Arc::new(map),
        }
    }
}

impl BlockRetriever for MockBlockRetriever {
    async fn get_block(&self, hash: &str) -> Result<GetBlockModel, ObserverError> {
        match self.blocks.get(hash) {
            Some(block) => Ok(block.clone()),
            None => Err(ObserverError::FailedToFetch(hash.to_string())),
        }
    }

    async fn get_tip_height(&self) -> Result<i64, ObserverError> {
        Ok(self.blocks.values().map(|b| b.height).max().unwrap_or(0))
    }

    async fn get_block_hash(&self, height: u64) -> Result<String, ObserverError> {
        self.blocks
            .values()
            .find(|b| b.height == height as i64)
            .map(|b| b.hash.clone())
            .ok_or_else(|| ObserverError::FailedToFetch(format!("no block at height {height}")))
    }
}

#[derive(Clone, Default)]
pub struct MockTransactionRetriever {
    txs_fetched: Arc<Mutex<Vec<String>>>,
    fail_for: Arc<Vec<String>>,
    not_found_for: Arc<Vec<String>>,
    invalid_for: Arc<Vec<String>>,
}

impl MockTransactionRetriever {
    /// Builds a retriever that returns a transient error for the given txids
    pub fn failing_for(txids: impl IntoIterator<Item = String>) -> Self {
        Self {
            fail_for: Arc::new(txids.into_iter().collect()),
            ..Self::default()
        }
    }

    /// Builds a retriever that returns `TxNotFoundInMempool` for the given txids
    pub fn not_found_for(txids: impl IntoIterator<Item = String>) -> Self {
        Self {
            not_found_for: Arc::new(txids.into_iter().collect()),
            ..Self::default()
        }
    }

    /// Builds a retriever that returns `InvalidParams` for the given txids,
    /// as the real one does for a txid that does not parse
    pub fn invalid_for(txids: impl IntoIterator<Item = String>) -> Self {
        Self {
            invalid_for: Arc::new(txids.into_iter().collect()),
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
        if self.not_found_for.iter().any(|t| t == txid) {
            return Err(ObserverError::TxNotFoundInMempool(txid.to_string()));
        }
        if self.fail_for.iter().any(|t| t == txid) {
            return Err(ObserverError::FailedToFetch(txid.to_string()));
        }
        if self.invalid_for.iter().any(|t| t == txid) {
            return Err(ObserverError::InvalidParams(txid.to_string()));
        }
        Ok(GetRawTransactionModel {
            txid: txid.to_string(),
            version: 0,
            lock_time: 0,
            vsize: 141,
            weight: 564,
            input_count: 1,
            input_txids: vec![format!("parent-of-{txid}")],
            output_count: 0,
            confirmations: 0,
            time: None,
        })
    }
}

#[derive(Clone, Default)]
pub struct MockClusterRetriever {
    clusters_fetched: Arc<Mutex<Vec<String>>>,
    seed_clusters: Arc<Mutex<HashMap<String, GetMempoolClusterModel>>>,
    strict: bool,
}

impl MockClusterRetriever {
    /// Builds a retriever that returns the given cluster for each of its member
    /// txids. A txid with no fixture comes back as a cluster of its own, which
    /// is what Core answers for a transaction with no relatives in the mempool.
    pub fn with_clusters(clusters: Vec<GetMempoolClusterModel>) -> Self {
        let retriever = Self::default();
        retriever.set_clusters(clusters);
        retriever
    }

    /// Like [`Self::with_clusters`], but a txid with no fixture is reported as
    /// gone from the mempool. For tests that have nothing to do with lone
    /// transactions: under the honest default they would persist a cluster for
    /// every txid they happen to touch.
    pub fn strict(clusters: Vec<GetMempoolClusterModel>) -> Self {
        Self {
            strict: true,
            ..Self::with_clusters(clusters)
        }
    }

    /// Replaces the fixtures, for tests that drive several sync rounds and need
    /// the node's answer to change between them.
    pub fn set_clusters(&self, clusters: Vec<GetMempoolClusterModel>) {
        let mut seed = self.seed_clusters.lock().unwrap();
        seed.clear();
        for cluster in clusters {
            for txid in &cluster.txids {
                seed.insert(txid.clone(), cluster.clone());
            }
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
        let seeded = self.seed_clusters.lock().unwrap().get(txid).cloned();
        match seeded {
            Some(cluster) => Ok(cluster),
            None if self.strict => Err(ObserverError::TxNotFoundInMempool(txid.to_string())),
            None => Ok(GetMempoolClusterModel {
                cluster_weight: 0,
                tx_count: 1,
                txids: vec![txid.to_string()],
                total_fee_sats: 0,
            }),
        }
    }
}

#[derive(Clone)]
pub struct MockNetworkRetriever {
    response: Arc<Mutex<Result<GetNetworkInfoModel, String>>>,
}

impl MockNetworkRetriever {
    pub fn with_subversion(subversion: &str) -> Self {
        Self {
            response: Arc::new(Mutex::new(Ok(GetNetworkInfoModel {
                version: 270_000,
                subversion: subversion.to_string(),
            }))),
        }
    }

    pub fn failing() -> Self {
        Self {
            response: Arc::new(Mutex::new(Err("getnetworkinfo unreachable".to_string()))),
        }
    }

    pub fn set_subversion(&self, subversion: &str) {
        *self.response.lock().unwrap() = Ok(GetNetworkInfoModel {
            version: 270_000,
            subversion: subversion.to_string(),
        });
    }
}

impl NetworkRetriever for MockNetworkRetriever {
    async fn get_network_info(&self) -> Result<GetNetworkInfoModel, ObserverError> {
        match &*self.response.lock().unwrap() {
            Ok(model) => Ok(model.clone()),
            Err(e) => Err(ObserverError::FailedToFetch(e.clone())),
        }
    }
}
