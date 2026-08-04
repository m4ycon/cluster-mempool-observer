use crate::infra::readiness::Readiness;
use observer::retrievers::ChainRetriever;
use std::time::Duration;

/// Polls until the node answers RPC and has left initial block download,
/// publishing what it sees to `readiness` so `/health` can report the wait.
pub async fn wait_until_ready<C: ChainRetriever>(
    chain: &C,
    readiness: &Readiness,
    poll_interval: Duration,
) {
    loop {
        match chain.get_blockchain_info().await {
            Ok(info) if !info.initial_block_download => {
                tracing::info!(
                    "node ready at height {} ({:.2}% verified)",
                    info.blocks,
                    info.verification_progress * 100.0
                );
                readiness.record_node(&info);
                return;
            }
            Ok(info) => {
                tracing::info!(
                    "waiting for node: syncing {}/{} blocks ({:.2}%)",
                    info.blocks,
                    info.headers,
                    info.verification_progress * 100.0
                );
                readiness.record_node(&info);
            }
            Err(e) => {
                tracing::warn!("waiting for node: not reachable yet: {e}");
                readiness.record_node_error(&e);
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use observer::error::ObserverError;
    use shared::models::GetBlockchainInfoModel;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Errors, then reports IBD, then reports ready -- one response per call.
    #[derive(Clone)]
    struct ScriptedNode {
        calls: Arc<AtomicUsize>,
    }

    impl ChainRetriever for ScriptedNode {
        async fn get_blockchain_info(&self) -> Result<GetBlockchainInfoModel, ObserverError> {
            match self.calls.fetch_add(1, Ordering::SeqCst) {
                0 => Err(ObserverError::FailedToFetch("down".into())),
                1 => Ok(info(true)),
                _ => Ok(info(false)),
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
    async fn waits_through_unreachable_and_ibd() {
        let node = ScriptedNode {
            calls: Arc::new(AtomicUsize::new(0)),
        };

        let readiness = Readiness::default();
        wait_until_ready(&node, &readiness, Duration::from_millis(1)).await;

        // Returned only once IBD cleared: one error, one syncing, one ready.
        assert_eq!(node.calls.load(Ordering::SeqCst), 3);

        // The last poll is what /health reports.
        let report = readiness.report();
        assert!(report.node.reachable);
        assert_eq!(report.node.initial_block_download, Some(false));
        // Waiting on the node is not the same as being ready to serve.
        assert!(!report.ready);
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

        let calls = Arc::new(AtomicUsize::new(0));
        let readiness = Readiness::default();
        wait_until_ready(
            &ReadyNode(calls.clone()),
            &readiness,
            Duration::from_secs(3600),
        )
        .await;

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
