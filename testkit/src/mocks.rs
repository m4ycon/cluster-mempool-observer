use observer::error::ObserverError;
use observer::retrievers::{
    BlockRetriever, ClusterRetriever, NetworkRetriever, TransactionRetriever,
};
use shared::models::{
    GetBlockModel, GetMempoolClusterModel, GetNetworkInfoModel, GetRawTransactionModel,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

#[derive(Clone, Default)]
pub struct MockBlockRetriever {
    blocks: Arc<HashMap<String, GetBlockModel>>,
    pause: Option<Arc<BlockRetrieverPause>>,
    /// Remaining `get_block` failures per hash.
    failures_left: Arc<Mutex<HashMap<String, u32>>>,
    blocks_fetched: Arc<Mutex<Vec<String>>>,
}

/// Holds `get_block` open so a test can act while `apply_block` is mid-flight.
///
/// Semaphores rather than `Notify` so a release that lands before anyone waits
/// is still there when the waiter arrives.
pub struct BlockRetrieverPause {
    entered: Semaphore,
    release: Semaphore,
}

impl Default for BlockRetrieverPause {
    fn default() -> Self {
        Self {
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
        }
    }
}

impl BlockRetrieverPause {
    /// Returns once `apply_block` is parked in retrieval, having confirmed nothing.
    pub async fn wait_entered(&self) {
        self.entered
            .acquire()
            .await
            .expect("pause semaphore never closed")
            .forget();
    }

    pub fn release(&self) {
        self.release.add_permits(1);
    }
}

impl MockBlockRetriever {
    /// Builds a retriever that returns each block keyed by its hash
    pub fn with_blocks(blocks: Vec<GetBlockModel>) -> Self {
        let map = blocks.into_iter().map(|b| (b.hash.clone(), b)).collect();
        Self {
            blocks: Arc::new(map),
            ..Self::default()
        }
    }

    /// Fails `get_block` for `hash` the first `times` calls, then serves it.
    pub fn failing(self, hash: &str, times: u32) -> Self {
        self.failures_left
            .lock()
            .unwrap()
            .insert(hash.to_string(), times);
        self
    }

    /// Every hash `get_block` was asked for, in call order, failed calls included.
    pub fn blocks_fetched(&self) -> Vec<String> {
        self.blocks_fetched.lock().unwrap().clone()
    }

    /// Parks every `get_block` until the returned handle releases it.
    pub fn pausing(mut self) -> (Self, Arc<BlockRetrieverPause>) {
        let pause = Arc::new(BlockRetrieverPause::default());
        self.pause = Some(Arc::clone(&pause));
        (self, pause)
    }
}

impl BlockRetriever for MockBlockRetriever {
    async fn get_block(&self, hash: &str) -> Result<GetBlockModel, ObserverError> {
        if let Some(pause) = &self.pause {
            pause.entered.add_permits(1);
            pause
                .release
                .acquire()
                .await
                .expect("pause semaphore never closed")
                .forget();
        }
        self.blocks_fetched.lock().unwrap().push(hash.to_string());
        if let Some(left) = self.failures_left.lock().unwrap().get_mut(hash)
            && *left > 0
        {
            *left -= 1;
            return Err(ObserverError::FailedToFetch(format!("{hash} (injected)")));
        }
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
