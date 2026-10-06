use crate::db::TransactionRepository;
use crate::db::models::BackfillStage;
use observer::error::ObserverError;
use observer::retrievers::{TransactionRetriever, TransactionRpcRetriever};
use shared::snapshot::MempoolLedger;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Notify, Semaphore, mpsc};

const MAX_CONCURRENT_BACKFILLS: usize = 4;

/// Transient-failure retries before a request whose tx has left our mempool
/// ledger is given up on. While the tx is still there the node still has it, so
/// the request is retried for as long as that stays true and no cap applies.
/// backoff = min(0.25 * 2^n, 60). With n = 8, the backoff hits the ceiling of 60s.
/// To reach 30min, we need 8 retries (63.75s) + 29 (29*60s).
const MAX_ATTEMPTS: u8 = 37;

/// Backoff before the first retry; doubles per attempt up to `MAX_RETRY_BACKOFF`.
const RETRY_BACKOFF_BASE: Duration = Duration::from_millis(250);

/// Ceiling on the retry backoff -- a down node should not be polled harder than this.
const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(60);

/// metrics: Txids sent for enrichment; retries are not counted again.
const TX_BACKFILL_ENQUEUED_TOTAL: &str = "tx_backfill_enqueued_total";

/// metrics: Rows successfully enriched from a node fetch, labelled by stage.
const TX_BACKFILL_TOTAL: &str = "tx_backfill_total";

/// metrics: Backfill requests, first tries and retries alike, dropped because the queue was full.
const TX_BACKFILL_QUEUE_DROPPED_TOTAL: &str = "tx_backfill_queue_dropped_total";

/// One txid queued for enrichment from the node, from `stage` onwards.
pub struct BackfillRequest {
    pub txid: String,
    pub attempts: u8,
    pub stage: BackfillStage,
}

impl BackfillRequest {
    pub fn new(txid: String, stage: BackfillStage) -> Self {
        Self {
            txid,
            attempts: 0,
            stage,
        }
    }
}

/// The tx backfill queue, owning both ends of the channel.
#[derive(Clone)]
pub struct TxBackfillQueue {
    sender: mpsc::Sender<BackfillRequest>,
    receiver: Arc<Mutex<Option<mpsc::Receiver<BackfillRequest>>>>,
}

impl TxBackfillQueue {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity);
        Self {
            sender,
            receiver: Arc::new(Mutex::new(Some(receiver))),
        }
    }

    pub fn enqueue(&self, txid: String, stage: BackfillStage) {
        if self.send(BackfillRequest::new(txid, stage)) {
            metrics::counter!(TX_BACKFILL_ENQUEUED_TOTAL).increment(1);
        }
    }

    /// Hands the consuming end over; yields it exactly once.
    pub fn take_receiver(&self) -> Option<mpsc::Receiver<BackfillRequest>> {
        self.receiver
            .lock()
            .expect("tx backfill queue poisoned")
            .take()
    }

    /// Re-enqueues a retry, preserving its attempt count.
    fn requeue(&self, req: BackfillRequest) {
        self.send(req);
    }

    /// Returns whether the request made it onto the queue.
    fn send(&self, req: BackfillRequest) -> bool {
        match self.sender.try_send(req) {
            Ok(()) => true,
            Err(TrySendError::Full(req)) => {
                tracing::warn!(
                    "tx_backfill_queue: queue full, dropping backfill request for {} (attempts: {})",
                    req.txid,
                    req.attempts
                );
                metrics::counter!(TX_BACKFILL_QUEUE_DROPPED_TOTAL).increment(1);
                false
            }
            // only once shutdown has closed the consuming end
            Err(TrySendError::Closed(req)) => {
                tracing::debug!(
                    "tx_backfill_queue: shutting down, dropping backfill request for {}",
                    req.txid
                );
                false
            }
        }
    }

    /// Resolves once the consuming end is closed, i.e. shutdown has begun and any
    /// further requeue would be dropped anyway.
    async fn closed(&self) {
        self.sender.closed().await;
    }
}

/// Event-driven consumer of the tx backfill queue: re-fetches each tx from the
/// node the moment it is enqueued -- while the node still has it in mempool --
/// and fills in whatever the mempool add path could not supply.
#[derive(Clone)]
pub struct TxBackfillConsumer<TR: TransactionRetriever = TransactionRpcRetriever> {
    transaction_repository: TransactionRepository,
    transaction_retriever: TR,
    queue: TxBackfillQueue,
    mempool_ledger: MempoolLedger,
}

impl<TR: TransactionRetriever + 'static> TxBackfillConsumer<TR> {
    pub fn new(
        transaction_repository: TransactionRepository,
        transaction_retriever: TR,
        queue: TxBackfillQueue,
        mempool_ledger: MempoolLedger,
    ) -> Self {
        Self {
            transaction_repository,
            transaction_retriever,
            queue,
            mempool_ledger,
        }
    }

    /// Drains the queue forever, capping concurrent fetches at
    /// `MAX_CONCURRENT_BACKFILLS`. On `shutdown`, stops accepting new sends but
    /// finishes the buffered backlog (and any in-flight fetches) before returning.
    pub async fn consume(&self, mut rx: mpsc::Receiver<BackfillRequest>, shutdown: Arc<Notify>) {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_BACKFILLS));
        let mut in_flight = tokio::task::JoinSet::new();

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    // stop accepting new sends; already-buffered items keep
                    // coming out of recv() until the backlog is empty
                    rx.close();
                }
                req = rx.recv() => {
                    let Some(req) = req else { break };
                    let permit = semaphore
                        .clone()
                        .acquire_owned()
                        .await
                        .expect("semaphore is never closed");
                    in_flight.spawn(self.clone().process(req, permit));
                }
                Some(_) = in_flight.join_next(), if !in_flight.is_empty() => {}
            }
        }

        while in_flight.join_next().await.is_some() {}
    }

    /// Runs one request from its stage onwards; releases `permit` before any retry
    /// backoff so a retrying request never holds a fetch slot hostage for the delay.
    /// A failed stage is retried on its own, never re-running the stages before it.
    async fn process(self, req: BackfillRequest, permit: tokio::sync::OwnedSemaphorePermit) {
        let outcome = self.run_stages(&req.txid, req.stage).await;
        drop(permit);

        let Err((stage, e)) = outcome else { return };
        match e {
            ObserverError::TxNotFoundInMempool(_) => {
                // terminal: confirmed or evicted before we got to it. The block
                // path, if any, already wrote the complete row -- nothing to do.
                tracing::info!(
                    "tx_backfill: {} no longer in the node's mempool at the {} stage, leaving it hollow",
                    req.txid,
                    stage.as_str()
                );
            }
            ObserverError::InvalidParams(e) => {
                // terminal: the txid does not parse, so no amount of retrying makes
                // the node answer. Whatever put it on the queue is the bug.
                tracing::warn!(
                    "tx_backfill: refusing to fetch malformed txid {}: {e}",
                    req.txid
                );
            }
            e => {
                let attempts_made = req.attempts.saturating_add(1);
                let in_mempool = self.mempool_ledger.contains(&req.txid);
                if should_retry(attempts_made, in_mempool) {
                    if attempts_made == MAX_ATTEMPTS {
                        tracing::warn!(
                            "tx_backfill: still failing the {} stage of {} after {attempts_made} attempts, \
                             retrying while it stays in the mempool: {e}",
                            stage.as_str(),
                            req.txid
                        );
                    } else {
                        tracing::debug!(
                            "tx_backfill: transient failure at the {} stage of {}, retrying: {e}",
                            stage.as_str(),
                            req.txid
                        );
                    }

                    // shutdown closes the queue, so a backoff still running then can
                    // only end in a dropped requeue -- abandon it and let the drain finish
                    tokio::select! {
                        _ = tokio::time::sleep(retry_backoff(req.attempts)) => {
                            self.queue.requeue(BackfillRequest {
                                txid: req.txid,
                                attempts: attempts_made,
                                stage,
                            });
                        }
                        _ = self.queue.closed() => {
                            tracing::debug!(
                                "tx_backfill: shutting down, dropping retry for {}",
                                req.txid
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        "tx_backfill: giving up on the {} stage of {} after {attempts_made} attempts: {e}",
                        stage.as_str(),
                        req.txid,
                    );
                }
            }
        }
    }

    async fn run_stages(
        &self,
        txid: &str,
        stage: BackfillStage,
    ) -> Result<(), (BackfillStage, ObserverError)> {
        let mut stage = Some(stage);
        if stage == Some(BackfillStage::Raw) {
            stage = self
                .run_raw_stage(txid)
                .await
                .map_err(|e| (BackfillStage::Raw, e))?;
        }
        if stage == Some(BackfillStage::Entry) {
            self.run_entry_stage(txid)
                .await
                .map_err(|e| (BackfillStage::Entry, e))?;
        }
        Ok(())
    }

    async fn run_raw_stage(&self, txid: &str) -> Result<Option<BackfillStage>, ObserverError> {
        let tx = self.transaction_retriever.get_raw_transaction(txid).await?;
        match self
            .transaction_repository
            .backfill_raw(txid, &tx.input_txids, tx.vsize as i64)
            .await
        {
            Ok(None) => {
                tracing::warn!("tx_backfill: fetched {txid} but no hollow row was left to fill");
                Ok(None)
            }
            Ok(Some(fill)) => {
                metrics::counter!(TX_BACKFILL_TOTAL, "stage" => BackfillStage::Raw.as_str())
                    .increment(1);
                Ok(fill.backfill_stage())
            }
            Err(e) => {
                tracing::error!("tx_backfill: failed to persist {txid}: {e}");
                Ok(None)
            }
        }
    }

    async fn run_entry_stage(&self, txid: &str) -> Result<Option<BackfillStage>, ObserverError> {
        let entry = self.transaction_retriever.get_mempool_entry(txid).await?;
        match self
            .transaction_repository
            .backfill_fee(txid, entry.fee_in_sats as i64)
            .await
        {
            Ok(None) => {
                // as in the raw stage: legitimate only if the block path got there first
                tracing::warn!(
                    "tx_backfill: fetched the fee of {txid} but no row was left without one"
                );
                Ok(None)
            }
            Ok(Some(fill)) => {
                metrics::counter!(TX_BACKFILL_TOTAL, "stage" => BackfillStage::Entry.as_str())
                    .increment(1);
                Ok(fill.backfill_stage())
            }
            Err(e) => {
                tracing::error!("tx_backfill: failed to persist the fee of {txid}: {e}");
                Ok(None)
            }
        }
    }
}

/// Whether a transiently-failed request is worth another fetch. A tx still in our
/// mempool ledger is still on the node, so no attempt cap applies to it -- only a
/// terminal answer from the node, or the tx leaving the ledger, ends the retries.
fn should_retry(attempts_made: u8, in_mempool: bool) -> bool {
    in_mempool || attempts_made < MAX_ATTEMPTS
}

/// Doubles per attempt up to the ceiling: 250ms, 500ms, 1s, 2s ... 60s.
fn retry_backoff(attempts: u8) -> Duration {
    let factor = 1u32.checked_shl(attempts as u32).unwrap_or(u32::MAX);
    RETRY_BACKOFF_BASE
        .saturating_mul(factor)
        .min(MAX_RETRY_BACKOFF)
}

#[cfg(test)]
mod queue_tests {
    use super::*;

    #[test]
    fn backfill_request_new_starts_at_zero_attempts() {
        assert_eq!(
            BackfillRequest::new("a".to_string(), BackfillStage::Raw).attempts,
            0
        );
    }

    #[test]
    fn retry_backoff_doubles_then_caps() {
        assert_eq!(retry_backoff(0), Duration::from_millis(250));
        assert_eq!(retry_backoff(1), Duration::from_millis(500));
        assert_eq!(retry_backoff(2), Duration::from_secs(1));
        assert_eq!(retry_backoff(7), Duration::from_secs(32));
        assert_eq!(retry_backoff(8), MAX_RETRY_BACKOFF);
        // no overflow past the shift width
        assert_eq!(retry_backoff(u8::MAX), MAX_RETRY_BACKOFF);
    }

    #[test]
    fn max_attempts_spans_half_an_hour_of_backoff() {
        // the final attempt is not followed by a backoff, so the window stops one short
        let window: Duration = (0..MAX_ATTEMPTS - 1).map(retry_backoff).sum();
        let half_hour = Duration::from_secs(30 * 60);
        assert!(window <= half_hour, "retry window overshoots: {window:?}");
        assert!(
            half_hour - window < MAX_RETRY_BACKOFF,
            "retry window falls more than one backoff step short: {window:?}"
        );
    }

    #[test]
    fn should_retry_respects_max_attempts_once_the_tx_left_the_mempool() {
        assert!(should_retry(MAX_ATTEMPTS - 1, false));
        assert!(!should_retry(MAX_ATTEMPTS, false));
    }

    #[test]
    fn should_retry_never_gives_up_while_the_tx_is_still_in_the_mempool() {
        assert!(should_retry(MAX_ATTEMPTS, true));
        assert!(should_retry(u8::MAX, true));
    }

    #[test]
    fn enqueue_never_blocks_and_drops_past_capacity() {
        let queue = TxBackfillQueue::new(1);
        let mut rx = queue.take_receiver().expect("receiver");

        let rendered = testkit::metrics::capture(async {
            queue.enqueue("a".to_string(), BackfillStage::Raw);
            queue.enqueue("b".to_string(), BackfillStage::Raw); // queue full, dropped rather than blocked
        });
        testkit::metrics::assert_series(&rendered, "tx_backfill_enqueued_total 1");
        testkit::metrics::assert_series(&rendered, "tx_backfill_queue_dropped_total 1");

        let first = rx.try_recv().expect("first enqueued txid available");
        assert_eq!(first.txid, "a");
        assert!(
            rx.try_recv().is_err(),
            "second txid should have been dropped, not queued"
        );
    }

    #[test]
    fn requeue_never_blocks_and_drops_past_capacity() {
        let queue = TxBackfillQueue::new(1);
        let mut rx = queue.take_receiver().expect("receiver");

        let rendered = testkit::metrics::capture(async {
            queue.requeue(BackfillRequest {
                txid: "a".to_string(),
                attempts: 3,
                stage: BackfillStage::Entry,
            });
            queue.requeue(BackfillRequest {
                txid: "b".to_string(),
                attempts: 1,
                stage: BackfillStage::Raw,
            });
        });
        testkit::metrics::assert_series(&rendered, "tx_backfill_queue_dropped_total 1");

        let first = rx.try_recv().expect("first requeued txid available");
        assert_eq!(
            (first.txid.as_str(), first.attempts, first.stage),
            ("a", 3, BackfillStage::Entry)
        );
        assert!(
            rx.try_recv().is_err(),
            "second retry should have been dropped, not queued"
        );
    }
}
