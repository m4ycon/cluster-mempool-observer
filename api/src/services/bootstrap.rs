use crate::db::models::{NewTransaction, SystemEventKind};
use crate::db::{MempoolDeltaRepository, TransactionRepository};
use crate::infra::config::ApiConfig;
use crate::services::block::BlockService;
use crate::services::cluster::ClusterService;
use crate::services::node_status::NodeStatusService;
use crate::services::system_event::SystemEventService;
use observer::clients::Clients;
use observer::retrievers::{MempoolRetriever, NetworkRpcRetriever};
use serde_json::json;
use shared::metrics::timed_async_with;
use shared::models::MempoolEntrySummary;
use shared::snapshot::{FeerateDiagramSnapshot, MempoolLedger, MempoolSnapshot};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const BOOTSTRAP_SECONDS: &str = "bootstrap_stage_seconds";

/// Txids reconciled between the persisted mempool state and the live node at startup.
pub struct MempoolReconciliation {
    pub txs_added: usize,
    pub txs_removed: usize,
}

#[derive(Clone)]
pub struct BootstrapService {
    mempool_delta_repository: MempoolDeltaRepository,
    mempool_retriever: MempoolRetriever,
    transaction_repository: TransactionRepository,
    mempool_ledger: MempoolLedger,
    block_service: BlockService,
    cluster_service: ClusterService,
    system_event_service: SystemEventService,
    node_status_service: NodeStatusService<NetworkRpcRetriever>,
}

impl BootstrapService {
    // Plain dependency wiring; splitting it would only move the count elsewhere.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mempool_delta_repository: MempoolDeltaRepository,
        mempool_retriever: MempoolRetriever,
        transaction_repository: TransactionRepository,
        mempool_ledger: MempoolLedger,
        block_service: BlockService,
        cluster_service: ClusterService,
        system_event_service: SystemEventService,
        node_status_service: NodeStatusService<NetworkRpcRetriever>,
    ) -> Self {
        Self {
            mempool_delta_repository,
            mempool_retriever,
            transaction_repository,
            mempool_ledger,
            block_service,
            cluster_service,
            system_event_service,
            node_status_service,
        }
    }

    pub async fn run(
        &self,
        cfg: &ApiConfig,
        clients: Clients,
        snapshot: MempoolSnapshot,
        feerate_diagram_snapshot: FeerateDiagramSnapshot,
    ) {
        let start = Instant::now();
        self.system_event_service
            .record(SystemEventKind::BootstrapStarted, json!({}))
            .await;

        // sync any blocks missed while the api was down
        timed_async_with(
            BOOTSTRAP_SECONDS,
            &[("stage", "sync_missing_blocks")],
            self.block_service.sync_missing_blocks(),
        )
        .await;

        // bootstrap the mempool state
        let reconciliation = timed_async_with(
            BOOTSTRAP_SECONDS,
            &[("stage", "mempool_snapshot")],
            self.setup_mempool_snapshot(&snapshot),
        )
        .await;

        // seed cluster snapshot from persisted active clusters
        timed_async_with(
            BOOTSTRAP_SECONDS,
            &[("stage", "seed_clusters")],
            self.cluster_service.seed_snapshot(),
        )
        .await;

        // spawn the block stream persister
        let block_service = self.block_service.clone();
        let block_stream = block_service.get_block_stream().await;
        tokio::spawn(async move { block_service.persist_blocks_and_txs(block_stream).await });

        // spawn the node status stream consumer
        let node_status_service = self.node_status_service.clone();
        let status_stream = node_status_service.get_status_stream().await;
        tokio::spawn(async move { node_status_service.consume(status_stream).await });

        // spawn the observer runner
        let observer_cfg = cfg.observer.clone();
        let mempool_ledger = self.mempool_ledger.clone();
        tokio::spawn(async move {
            observer::runner::run(
                &observer_cfg,
                clients,
                mempool_ledger,
                snapshot,
                feerate_diagram_snapshot,
            )
            .await
        });

        self.system_event_service
            .record(
                SystemEventKind::BootstrapCompleted,
                bootstrap_completed_details(start.elapsed(), reconciliation),
            )
            .await;
    }

    /// Compares persisted mempool state against the live node at startup: seeds the
    /// ledger's `live` with what the database already believes (no journal entries,
    /// those rows are already written), then submits the live mempool as authoritative
    /// so the ledger -- and, once it ticks, the reconciler -- produces exactly the
    /// add/remove delta between the two. Also seeds `snapshot` so the watcher
    /// (spawned after this) starts from the live baseline instead of reporting the
    /// whole mempool as `added`.
    pub async fn setup_mempool_snapshot(
        &self,
        snapshot: &MempoolSnapshot,
    ) -> Option<MempoolReconciliation> {
        let prev = match self.mempool_delta_repository.reconstruct_snapshot().await {
            Ok(set) => set,
            Err(e) => {
                tracing::error!("bootstrap: failed to reconstruct mempool snapshot: {e}");
                return None;
            }
        };

        let txs_verbose = match self.mempool_retriever.get_raw_mempool_verbose().await {
            Ok(model) => model,
            Err(e) => {
                tracing::error!("bootstrap: failed to fetch live mempool: {e:?}");
                return None;
            }
        };

        let entries: HashMap<String, MempoolEntrySummary> = txs_verbose
            .entries
            .into_iter()
            .map(|e| (e.txid.clone(), e))
            .collect();
        let live: HashSet<String> = entries.keys().cloned().collect();

        let mut added: Vec<String> = live.difference(&prev).cloned().collect();
        let mut removed: Vec<String> = prev.difference(&live).cloned().collect();
        added.sort();
        removed.sort();

        let reconciliation = MempoolReconciliation {
            txs_added: added.len(),
            txs_removed: removed.len(),
        };

        if !added.is_empty() || !removed.is_empty() {
            tracing::info!(
                "bootstrap: reconciling mempool ({} added, {} removed since last run)",
                added.len(),
                removed.len()
            );
        }

        // The reconciler only ever learns about a txid through the journal, so
        // fee/vsize from `getrawmempool verbose` would never reach it; upsert
        // those rows ourselves, or every added txid lands hollow and queues for
        // backfill. `insert_many` is on_conflict do_nothing, so this never
        // touches a row that already exists -- and it never touches
        // `mempool_deltas`, which stays the reconciler's alone.
        let new_rows: Vec<NewTransaction> = added
            .iter()
            .filter_map(|txid| entries.get(txid).map(NewTransaction::from))
            .collect();
        if let Err(e) = self.transaction_repository.insert_many(&new_rows).await {
            tracing::error!("bootstrap: failed to upsert live mempool transactions: {e}");
        }

        self.mempool_ledger.seed(prev);
        self.mempool_ledger.submit_authoritative(live.clone());

        snapshot.store(live);
        Some(reconciliation)
    }
}

fn bootstrap_completed_details(
    duration: Duration,
    reconciliation: Option<MempoolReconciliation>,
) -> serde_json::Value {
    match reconciliation {
        Some(r) => json!({
            "duration_ms": duration.as_millis(),
            "txs_added": r.txs_added,
            "txs_removed": r.txs_removed,
        }),
        None => json!({
            "duration_ms": duration.as_millis(),
            "reconciled": false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_completed_details_reports_counts_when_reconciled() {
        let details = bootstrap_completed_details(
            Duration::from_millis(250),
            Some(MempoolReconciliation {
                txs_added: 3,
                txs_removed: 1,
            }),
        );

        assert_eq!(details["duration_ms"], 250);
        assert_eq!(details["txs_added"], 3);
        assert_eq!(details["txs_removed"], 1);
        assert!(details.get("reconciled").is_none());
    }

    #[test]
    fn bootstrap_completed_details_reports_unreconciled_instead_of_zero_counts() {
        let details = bootstrap_completed_details(Duration::from_millis(50), None);

        assert_eq!(details["duration_ms"], 50);
        assert_eq!(details["reconciled"], false);
        assert!(details.get("txs_added").is_none());
        assert!(details.get("txs_removed").is_none());
    }
}
