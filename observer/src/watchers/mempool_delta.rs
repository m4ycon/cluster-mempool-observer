use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use crate::watchers::watcher_trait::{
    WATCHER_POLL_ERRORS_TOTAL, WATCHER_POLL_OVERRUNS_TOTAL, WATCHER_POLL_SECONDS,
};
use corepc_client::types::model::GetRawMempool;
use shared::metrics::record_duration;
use shared::snapshot::MempoolLedger;
use shared::subjects::Subject;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Polls `getrawmempool` and feeds the result to the `MempoolLedger`.
pub struct MempoolDeltaWatcher {
    rpc: RpcClient,
    watch_rate: u32,
    ledger: MempoolLedger,
}

impl MempoolDeltaWatcher {
    pub fn new(rpc: RpcClient, watch_rate: u32, ledger: MempoolLedger) -> Self {
        Self {
            rpc,
            watch_rate,
            ledger,
        }
    }

    async fn fetch(&self) -> Result<GetRawMempool, ObserverError> {
        self.rpc
            .call("getrawmempool", |client| client.get_raw_mempool())
            .await?
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))
    }

    /// One poll cycle: fetch, hand the live set to the ledger. Returns how
    /// long it took so the caller can subtract it from the sleep.
    async fn poll_once(&self) -> Duration {
        let subject = Subject::MempoolDelta;
        let started = Instant::now();

        match self.fetch().await {
            Ok(response) => {
                let txids: HashSet<String> =
                    response.0.iter().map(|txid| txid.to_string()).collect();
                self.ledger.submit_authoritative(txids);
            }
            Err(e) => {
                metrics::counter!(WATCHER_POLL_ERRORS_TOTAL, "subject" => subject.as_str())
                    .increment(1);
                tracing::error!("watcher error on {subject}: {e:?}");
            }
        }

        let elapsed = started.elapsed();
        record_duration(
            WATCHER_POLL_SECONDS,
            &[("subject", subject.as_str())],
            elapsed,
        );
        elapsed
    }

    /// Polls at `watch_rate` forever. Spawn it on its own task.
    pub async fn run(self) {
        let rate = Duration::from_secs(self.watch_rate as u64);
        let subject = Subject::MempoolDelta;
        loop {
            let elapsed = self.poll_once().await;
            if elapsed < rate {
                sleep(rate - elapsed).await;
            } else {
                metrics::counter!(WATCHER_POLL_OVERRUNS_TOTAL, "subject" => subject.as_str())
                    .increment(1);
            }
        }
    }
}
