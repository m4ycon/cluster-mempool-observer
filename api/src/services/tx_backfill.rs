use crate::db::TransactionRepository;
use observer::error::ObserverError;
use observer::retrievers::{TransactionRetriever, TransactionRpcRetriever};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, Semaphore, mpsc};

const MAX_CONCURRENT_BACKFILLS: usize = 4;

/// Transient-failure retries before a request is given up on.
const MAX_ATTEMPTS: u8 = 3;

/// Backoff before a transiently-failed request is retried.
const RETRY_BACKOFF: Duration = Duration::from_millis(100);

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
}

/// Event-driven consumer of the tx backfill queue: re-fetches each tx from the
/// node the moment it is enqueued -- while the node still has it in mempool --
/// and fills in whatever the mempool add path could not supply.
#[derive(Clone)]
pub struct TxBackfillConsumer<TR: TransactionRetriever = TransactionRpcRetriever> {
    transaction_repository: TransactionRepository,
    transaction_retriever: TR,
    requeue: TxBackfillQueue,
}

impl<TR: TransactionRetriever + 'static> TxBackfillConsumer<TR> {
    pub fn new(
        transaction_repository: TransactionRepository,
        transaction_retriever: TR,
        requeue: TxBackfillQueue,
    ) -> Self {
        Self {
            transaction_repository,
            transaction_retriever,
            requeue,
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
                    ));
                }
                Some(_) = in_flight.join_next(), if !in_flight.is_empty() => {}
            }
        }

        while in_flight.join_next().await.is_some() {}
    }
}

/// Fetches and persists one request; releases `permit` before any retry backoff
/// so a retrying request never holds a fetch slot hostage for the delay.
async fn process<TR: TransactionRetriever>(
    req: BackfillRequest,
    permit: tokio::sync::OwnedSemaphorePermit,
    transaction_repository: TransactionRepository,
    transaction_retriever: TR,
    requeue: TxBackfillQueue,
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
            if req.attempts + 1 < MAX_ATTEMPTS {
                tracing::warn!(
                    "tx_backfill: transient failure fetching {}, retrying: {e}",
                    req.txid
                );
                tokio::time::sleep(RETRY_BACKOFF).await;
                requeue.requeue(BackfillRequest {
                    txid: req.txid,
                    attempts: req.attempts + 1,
                });
            } else {
                tracing::warn!(
                    "tx_backfill: giving up on {} after {} attempts: {e}",
                    req.txid,
                    req.attempts + 1
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
