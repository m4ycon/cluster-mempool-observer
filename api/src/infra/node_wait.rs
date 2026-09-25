use crate::infra::readiness::Readiness;
use crate::services::node_health::NodeHealthReporter;
use observer::retrievers::{ChainRetriever, MempoolRetriever};
use std::time::Duration;

/// Polls until the node answers RPC, has left initial block download and has
/// finished loading `mempool.dat`, publishing what it sees to `readiness` so
/// `/health` can report the wait, and to "health reporter" so the system-events
/// log records connect/disconnect transitions.
pub async fn wait_until_ready<C: ChainRetriever, M: MempoolRetriever, H: NodeHealthReporter>(
    chain: &C,
    mempool: &M,
    health: &H,
    readiness: &Readiness,
    poll_interval: Duration,
) {
    loop {
        match chain.get_blockchain_info().await {
            Ok(info) if !info.initial_block_download => {
                readiness.record_node(&info);
                health.observe_reachable(&info).await;

                match mempool.get_mempool_info().await {
                    Ok(mempool) if mempool.loaded => {
                        tracing::info!(
                            "node ready at height {} ({:.2}% verified)",
                            info.blocks,
                            info.verification_progress * 100.0
                        );
                        readiness.record_mempool_loaded(true);
                        return;
                    }
                    Ok(_) => {
                        tracing::info!("waiting for node: mempool still loading");
                        readiness.record_mempool_loaded(false);
                    }
                    Err(e) => tracing::warn!("waiting for node: getmempoolinfo failed: {e}"),
                }
            }
            Ok(info) => {
                tracing::info!(
                    "waiting for node: syncing {}/{} blocks ({:.2}%)",
                    info.blocks,
                    info.headers,
                    info.verification_progress * 100.0
                );
                readiness.record_node(&info);
                health.observe_reachable(&info).await;
            }
            Err(e) => {
                tracing::warn!("waiting for node: not reachable yet: {e}");
                readiness.record_node_error(&e);
                health.observe_unreachable(&e).await;
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use observer::error::ObserverError;
    use shared::models::{GetBlockchainInfoModel, GetMempoolInfoModel, GetRawMempoolVerboseModel};
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// Records each `observe_*` call as "reachable" or "unreachable", in order.
    #[derive(Clone, Default)]
    struct RecordingHealth {
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl RecordingHealth {
        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl NodeHealthReporter for RecordingHealth {
        async fn observe_reachable(&self, _info: &GetBlockchainInfoModel) {
            self.calls.lock().unwrap().push("reachable");
        }

        async fn observe_unreachable(&self, _error: &ObserverError) {
            self.calls.lock().unwrap().push("unreachable");
        }
    }

    /// `getblockchaininfo` errors, then reports IBD, then reports ready.
    /// `getmempoolinfo` errors, then reports still loading, then reports loaded.
    /// One response per call.
    #[derive(Clone, Default)]
    struct ScriptedNode {
        chain_calls: Arc<AtomicUsize>,
        mempool_calls: Arc<AtomicUsize>,
    }

    impl ChainRetriever for ScriptedNode {
        async fn get_blockchain_info(&self) -> Result<GetBlockchainInfoModel, ObserverError> {
            match self.chain_calls.fetch_add(1, Ordering::SeqCst) {
                0 => Err(ObserverError::FailedToFetch("down".into())),
                1 => Ok(info(true)),
                _ => Ok(info(false)),
            }
        }
    }

    impl MempoolRetriever for ScriptedNode {
        async fn get_raw_mempool_verbose(
            &self,
        ) -> Result<GetRawMempoolVerboseModel, ObserverError> {
            unimplemented!("startup readiness only asks getmempoolinfo")
        }

        async fn get_mempool_txids(&self) -> Result<HashSet<String>, ObserverError> {
            unimplemented!("startup readiness only asks getmempoolinfo")
        }

        async fn get_mempool_info(&self) -> Result<GetMempoolInfoModel, ObserverError> {
            match self.mempool_calls.fetch_add(1, Ordering::SeqCst) {
                0 => Err(ObserverError::FailedToFetch("warming up".into())),
                1 => Ok(GetMempoolInfoModel { loaded: false }),
                _ => Ok(GetMempoolInfoModel { loaded: true }),
            }
        }
    }

    fn info(initial_block_download: bool) -> GetBlockchainInfoModel {
        GetBlockchainInfoModel {
            blocks: 100,
            headers: 100,
            verification_progress: 1.0,
            initial_block_download,
        }
    }

    #[tokio::test]
    async fn waits_through_unreachable_ibd_and_mempool_load() {
        let node = ScriptedNode::default();

        let readiness = Readiness::default();
        let health = RecordingHealth::default();
        wait_until_ready(&node, &node, &health, &readiness, Duration::from_millis(1)).await;

        // Returned only once the mempool loaded: one error, one syncing, then
        // three synced polls whose mempool answer was an error, loading, loaded.
        assert_eq!(node.chain_calls.load(Ordering::SeqCst), 5);
        // No mempool question while the node was down or syncing.
        assert_eq!(node.mempool_calls.load(Ordering::SeqCst), 3);

        // The last poll is what /health reports.
        let report = readiness.report();
        assert!(report.node.reachable);
        assert_eq!(report.node.initial_block_download, Some(false));
        assert_eq!(report.node.mempool_loaded, Some(true));
        // Waiting on the node is not the same as being ready to serve.
        assert!(!report.ready);

        // Every chain poll is reported to the health detector, in order; the
        // failed getmempoolinfo is not reported as a disconnect.
        assert_eq!(
            health.calls(),
            vec![
                "unreachable",
                "reachable",
                "reachable",
                "reachable",
                "reachable"
            ]
        );
    }

    #[tokio::test]
    async fn returns_immediately_when_already_synced() {
        #[derive(Clone)]
        struct ReadyNode(Arc<AtomicUsize>);

        impl ChainRetriever for ReadyNode {
            async fn get_blockchain_info(&self) -> Result<GetBlockchainInfoModel, ObserverError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(info(false))
            }
        }

        impl MempoolRetriever for ReadyNode {
            async fn get_raw_mempool_verbose(
                &self,
            ) -> Result<GetRawMempoolVerboseModel, ObserverError> {
                unimplemented!("startup readiness only asks getmempoolinfo")
            }

            async fn get_mempool_txids(&self) -> Result<HashSet<String>, ObserverError> {
                unimplemented!("startup readiness only asks getmempoolinfo")
            }

            async fn get_mempool_info(&self) -> Result<GetMempoolInfoModel, ObserverError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(GetMempoolInfoModel { loaded: true })
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let readiness = Readiness::default();
        let health = RecordingHealth::default();
        let node = ReadyNode(calls.clone());
        wait_until_ready(&node, &node, &health, &readiness, Duration::from_secs(3600)).await;

        // One getblockchaininfo and one getmempoolinfo.
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(health.calls(), vec!["reachable"]);
    }
}
