use crate::db::MempoolDeltaRepository;
use crate::infra::config::ApiConfig;
use crate::services::block::BlockService;
use crate::services::cluster::ClusterService;
use crate::services::mempool::MempoolService;
use observer::clients::Clients;
use observer::retrievers::{
    ClusterRetriever, ClusterRpcRetriever, MempoolRetriever, TransactionRetriever,
    TransactionRpcRetriever,
};
use shared::events::MempoolDeltaEvent;
use shared::snapshot::MempoolSnapshot;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub struct BootstrapService<
    TR: TransactionRetriever = TransactionRpcRetriever,
    CR: ClusterRetriever = ClusterRpcRetriever,
> {
    mempool_delta_repository: MempoolDeltaRepository,
    mempool_retriever: MempoolRetriever,
    mempool_service: MempoolService<TR, CR>,
    block_service: BlockService,
    cluster_service: ClusterService,
}

impl<TR: TransactionRetriever + 'static, CR: ClusterRetriever + 'static> BootstrapService<TR, CR> {
    pub fn new(
        mempool_delta_repository: MempoolDeltaRepository,
        mempool_retriever: MempoolRetriever,
        mempool_service: MempoolService<TR, CR>,
        block_service: BlockService,
        cluster_service: ClusterService,
    ) -> Self {
        Self {
            mempool_delta_repository,
            mempool_retriever,
            mempool_service,
            block_service,
            cluster_service,
        }
    }

    pub async fn run(&self, cfg: &ApiConfig, clients: Clients, snapshot: MempoolSnapshot) {
        // sync any blocks missed while the api was down
        self.block_service.sync_missing_blocks().await;

        // bootstrap the mempool state
        self.setup_mempool_snapshot(&snapshot).await;

        // seed cluster snapshot from persisted active clusters
        self.cluster_service.seed_snapshot().await;

        // spawn the block stream persister
        let block_service = self.block_service.clone();
        let block_stream = block_service.get_block_stream().await;
        tokio::spawn(async move { block_service.persist_blocks_and_txs(block_stream).await });

        // spawn the mempool delta stream persister
        let mempool_service = self.mempool_service.clone();
        let delta_stream = mempool_service.get_delta_stream().await;
        tokio::spawn(async move {
            mempool_service
                .persist_deltas_and_new_txs(delta_stream)
                .await
        });

        // spawn the observer runner
        let observer_cfg = cfg.observer.clone();
        tokio::spawn(async move { observer::runner::run(&observer_cfg, clients, snapshot).await });
    }

    /// Compares persisted mempool state against the live node at startup: diffs the
    /// reconstructed set vs the live mempool, records the difference as one delta, and
    /// seeds `snapshot` so the watcher (spawned after this) starts from the live baseline
    /// instead of reporting the whole mempool as `added`.
    pub async fn setup_mempool_snapshot(&self, snapshot: &MempoolSnapshot) {
        let prev = match self.mempool_delta_repository.reconstruct_snapshot().await {
            Ok(set) => set,
            Err(e) => {
                tracing::error!("bootstrap: failed to reconstruct mempool snapshot: {e}");
                return;
            }
        };

        let txs_verbose = match self.mempool_retriever.get_raw_mempool_verbose().await {
            Ok(model) => model,
            Err(e) => {
                tracing::error!("bootstrap: failed to fetch live mempool: {e:?}");
                return;
            }
        };

        let live: HashSet<String> = txs_verbose.entries.iter().map(|e| e.txid.clone()).collect();
        let fees: HashMap<String, i64> = txs_verbose
            .entries
            .iter()
            .map(|e| (e.txid.clone(), e.fee_in_sats as i64))
            .collect();

        let mut added: Vec<String> = live.difference(&prev).cloned().collect();
        let mut removed: Vec<String> = prev.difference(&live).cloned().collect();
        added.sort();
        removed.sort();

        if !added.is_empty() || !removed.is_empty() {
            tracing::info!(
                "bootstrap: reconciling mempool ({} added, {} removed since last run)",
                added.len(),
                removed.len()
            );
            let delta = MempoolDeltaEvent { added, removed };
            self.mempool_service
                .apply_bootstrap_delta(delta, fees)
                .await;
        }

        snapshot.store(live);
    }
}
