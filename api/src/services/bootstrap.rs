use crate::db::MempoolDeltaRepository;
use crate::services::mempool::MempoolService;
use observer::retrievers::{MempoolRetriever, TransactionRetriever};
use observer::snapshot::MempoolSnapshot;
use shared::events::MempoolDeltaEvent;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub struct BootstrapService<R: TransactionRetriever + Clone> {
    mempool_delta_repository: MempoolDeltaRepository,
    mempool_retriever: MempoolRetriever,
    mempool_service: MempoolService<R>,
}

impl<R: TransactionRetriever + Clone + 'static> BootstrapService<R> {
    pub fn new(
        mempool_delta_repository: MempoolDeltaRepository,
        mempool_retriever: MempoolRetriever,
        mempool_service: MempoolService<R>,
    ) -> Self {
        Self {
            mempool_delta_repository,
            mempool_retriever,
            mempool_service,
        }
    }

    /// Compares persisted mempool state against the live node at startup: diffs the
    /// reconstructed set vs the live mempool, records the difference as one delta, and
    /// seeds `snapshot` so the watcher (spawned after this) starts from the live baseline
    /// instead of reporting the whole mempool as `added`.
    pub async fn run(&self, snapshot: &MempoolSnapshot) {
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

        // TODO: make mempool cluster call for those who have ancestors or descendants
    }
}
