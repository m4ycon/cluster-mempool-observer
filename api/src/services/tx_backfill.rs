use crate::db::TransactionRepository;
use observer::error::ObserverError;
use observer::retrievers::{TransactionRetriever, TransactionRpcRetriever};
use shared::snapshot::MempoolSnapshot;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, Semaphore, mpsc};

const MAX_CONCURRENT_BACKFILLS: usize = 4;

/// Transient-failure retries before a request whose tx has left our mempool
/// snapshot is given up on. While the tx is still there the node still has it,
/// so this cap does not apply -- `HARD_MAX_ATTEMPTS` does.
const MAX_ATTEMPTS: u8 = 3;

/// Cap for the number of attempts to fetch a tx that is still in our mempool snapshot.
/// backoff = min(0.25 * 2^n, 60). With n = 8, the backoff hits the ceiling of 60s.
/// To reach 30min, we need 8 retries (63.75s) + 29 (29*60s).
const HARD_MAX_ATTEMPTS: u8 = 37;

/// Backoff before the first retry; doubles per attempt up to `MAX_RETRY_BACKOFF`.
const RETRY_BACKOFF_BASE: Duration = Duration::from_millis(250);

/// Ceiling on the retry backoff -- a down node should not be polled harder than this.
const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(60);

/// metrics: Rows successfully enriched from a node fetch.
const TX_BACKFILL_TOTAL: &str = "tx_backfill_total";

/// metrics: Backfill requests dropped because the queue was full.
const TX_BACKFILL_QUEUE_DROPPED_TOTAL: &str = "tx_backfill_queue_dropped_total";

/// One txid queued for enrichment from the node.
pub struct BackfillRequest {
    pub txid: String,
    pub attempts: u8,
}

impl BackfillRequest {
    pub fn new(txid: String) -> Self {
        Self { txid, attempts: 0 }
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

    pub fn enqueue(&self, txid: String) {
        if let Err(e) = self.sender.try_send(BackfillRequest::new(txid)) {
            tracing::warn!("tx_backfill_queue: dropping backfill request, queue full: {e}");
            metrics::counter!(TX_BACKFILL_QUEUE_DROPPED_TOTAL).increment(1);
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
        let _ = self.sender.try_send(req);
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
    requeue: TxBackfillQueue,
    mempool_snapshot: MempoolSnapshot,
}

impl<TR: TransactionRetriever + 'static> TxBackfillConsumer<TR> {
    pub fn new(
        transaction_repository: TransactionRepository,
        transaction_retriever: TR,
        requeue: TxBackfillQueue,
        mempool_snapshot: MempoolSnapshot,
    ) -> Self {
        Self {
            transaction_repository,
            transaction_retriever,
            requeue,
            mempool_snapshot,
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
                    in_flight.spawn(process(
                        req,
                        permit,
                        self.transaction_repository.clone(),
                        self.transaction_retriever.clone(),
                        self.requeue.clone(),
                        self.mempool_snapshot.clone(),
                    ));
                }
                Some(_) = in_flight.join_next(), if !in_flight.is_empty() => {}
            }
        }

        while in_flight.join_next().await.is_some() {}
    }
}

/// Whether a transiently-failed request is worth another fetch. A tx still in our
/// mempool snapshot is still on the node, so the attempt cap does not apply to it --
/// only `HARD_MAX_ATTEMPTS` does.
fn should_retry(attempts_made: u8, in_mempool: bool) -> bool {
    attempts_made < MAX_ATTEMPTS || (in_mempool && attempts_made < HARD_MAX_ATTEMPTS)
}

/// Doubles per attempt up to the ceiling: 250ms, 500ms, 1s, 2s ... 60s.
fn retry_backoff(attempts: u8) -> Duration {
    let factor = 1u32.checked_shl(attempts as u32).unwrap_or(u32::MAX);
    RETRY_BACKOFF_BASE
        .saturating_mul(factor)
        .min(MAX_RETRY_BACKOFF)
}

/// Fetches and persists one request; releases `permit` before any retry backoff
/// so a retrying request never holds a fetch slot hostage for the delay.
async fn process<TR: TransactionRetriever>(
    req: BackfillRequest,
    permit: tokio::sync::OwnedSemaphorePermit,
    transaction_repository: TransactionRepository,
    transaction_retriever: TR,
    requeue: TxBackfillQueue,
    mempool_snapshot: MempoolSnapshot,
) {
    let tx = match transaction_retriever.get_raw_transaction(&req.txid).await {
        Ok(tx) => tx,
        Err(ObserverError::TxNotFoundInMempool(_)) => {
            drop(permit);
            // terminal: confirmed or evicted before we got to it. The block
            // path, if any, already wrote the complete row -- nothing to do.
            tracing::debug!("tx_backfill: {} no longer in mempool", req.txid);
            return;
        }
        Err(e) => {
            drop(permit);
            let attempts_made = req.attempts + 1;
            if should_retry(attempts_made, mempool_snapshot.contains(&req.txid)) {
                tracing::debug!(
                    "tx_backfill: transient failure fetching {}, retrying: {e}",
                    req.txid
                );

                // shutdown closes the queue, so a backoff still running then can
                // only end in a dropped requeue -- abandon it and let the drain finish
                tokio::select! {
                    _ = tokio::time::sleep(retry_backoff(req.attempts)) => {
                        requeue.requeue(BackfillRequest {
                            txid: req.txid,
                            attempts: attempts_made,
                        });
                    }
                    _ = requeue.closed() => {
                        tracing::debug!(
                            "tx_backfill: shutting down, dropping retry for {}",
                            req.txid
                        );
                    }
                }
            } else {
                tracing::warn!(
                    "tx_backfill: giving up on {} after {attempts_made} attempts: {e}",
                    req.txid,
                );
            }
            return;
        }
    };

    match transaction_repository
        .backfill_from_fetch(&req.txid, &tx.input_txids, tx.vsize as i64)
        .await
    {
        Ok(_) => metrics::counter!(TX_BACKFILL_TOTAL).increment(1),
        Err(e) => tracing::error!("tx_backfill: failed to persist {}: {e}", req.txid),
    }
    drop(permit);
}

#[cfg(test)]
mod queue_tests {
    use super::*;

    #[test]
    fn backfill_request_new_starts_at_zero_attempts() {
        assert_eq!(BackfillRequest::new("a".to_string()).attempts, 0);
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
    fn should_retry_respects_max_attempts_once_the_tx_left_the_mempool() {
        assert!(should_retry(MAX_ATTEMPTS - 1, false));
        assert!(!should_retry(MAX_ATTEMPTS, false));
    }

    #[test]
    fn should_retry_ignores_max_attempts_while_the_tx_is_still_in_the_mempool() {
        assert!(should_retry(MAX_ATTEMPTS, true));
        assert!(should_retry(HARD_MAX_ATTEMPTS - 1, true));
    }

    #[test]
    fn should_retry_stops_at_the_hard_cap_even_in_the_mempool() {
        assert!(!should_retry(HARD_MAX_ATTEMPTS, true));
    }

    #[tokio::test]
    async fn enqueue_never_blocks_and_drops_past_capacity() {
        let queue = TxBackfillQueue::new(1);
        let mut rx = queue.take_receiver().expect("receiver");

        queue.enqueue("a".to_string());
        queue.enqueue("b".to_string()); // queue full, dropped rather than blocked

        let first = rx.try_recv().expect("first enqueued txid available");
        assert_eq!(first.txid, "a");
        assert!(
            rx.try_recv().is_err(),
            "second txid should have been dropped, not queued"
        );
    }
}
