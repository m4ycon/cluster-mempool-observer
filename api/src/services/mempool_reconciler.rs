use crate::db::{FlushOutcome, MempoolLedgerRepository};
use crate::services::cluster::ClusterService;
use crate::services::pubsub::PubSubService;
use crate::services::tx_backfill::TxBackfillQueue;
use observer::retrievers::{ClusterRetriever, ClusterRpcRetriever};
use shared::events::MempoolDeltaEvent;
use shared::metrics::timed_async;
use shared::snapshot::MempoolLedger;
use shared::subjects::Subject;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::MissedTickBehavior;

const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Journal entries not yet flushed, sampled once per tick.
const MEMPOOL_LEDGER_PENDING_LEN: &str = "mempool_ledger_pending_len";

/// 1 when the journal has ever overflowed and lost entries, 0 otherwise.
const MEMPOOL_LEDGER_DEGRADED: &str = "mempool_ledger_degraded";

/// One flush transaction (`write_batch` only).
const MEMPOOL_LEDGER_FLUSH_SECONDS: &str = "mempool_ledger_flush_seconds";

/// Txids seen entering and leaving the mempool.
const MDELTA_TXS_TOTAL: &str = "mempool_delta_txs_total";

/// Txids that were not already stored, so they were inserted hollow.
const MDELTA_NEW_TXS_TOTAL: &str = "mempool_new_txs_total";

/// Failures at any step of a tick, by `stage`. Each one used to be visible
/// only as a log line.
const MEMPOOL_PERSIST_FAILED_TOTAL: &str = "mempool_persist_failed_total";

/// The single writer of `mempool_deltas`: drains the `MempoolLedger` journal
/// and is the only thing that turns a journal entry into a database row.
#[derive(Clone)]
pub struct MempoolReconciler<CR: ClusterRetriever = ClusterRpcRetriever> {
    ledger: MempoolLedger,
    repository: MempoolLedgerRepository,
    tx_backfill_queue: TxBackfillQueue,
    cluster_service: ClusterService<CR>,
    pubsub: PubSubService,
}

impl<CR: ClusterRetriever> MempoolReconciler<CR> {
    pub fn new(
        ledger: MempoolLedger,
        repository: MempoolLedgerRepository,
        tx_backfill_queue: TxBackfillQueue,
        cluster_service: ClusterService<CR>,
        pubsub: PubSubService,
    ) -> Self {
        Self {
            ledger,
            repository,
            tx_backfill_queue,
            cluster_service,
            pubsub,
        }
    }

    /// Ticks forever until `shutdown` fires.
    pub async fn run(&self, shutdown: Arc<Notify>) {
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = shutdown.notified() => return,
                _ = interval.tick() => {}
            }
            self.tick().await;
        }
    }

    /// Flushes one journal batch.
    pub async fn tick(&self) {
        metrics::gauge!(MEMPOOL_LEDGER_PENDING_LEN).set(self.ledger.pending_len() as f64);
        metrics::gauge!(MEMPOOL_LEDGER_DEGRADED).set(if self.ledger.is_degraded() {
            1.0
        } else {
            0.0
        });

        let Some(batch) = self.ledger.begin_flush() else {
            return;
        };

        let outcome = timed_async(
            MEMPOOL_LEDGER_FLUSH_SECONDS,
            self.repository.write_batch(batch.entries()),
        )
        .await;

        let outcome: FlushOutcome = match outcome {
            Ok(outcome) => outcome,
            Err(e) => {
                // No rollback needed: nothing committed to the journal yet, so
                // the same prefix comes back on the next tick.
                metrics::counter!(MEMPOOL_PERSIST_FAILED_TOTAL, "stage" => "write_batch")
                    .increment(1);
                tracing::error!(
                    "mempool reconciler: failed to write batch, retrying next tick: {e}"
                );
                return;
            }
        };

        // Read out before commit consumes the batch.
        let (added_count, removed_count) = batch.transition_counts();
        let added = batch.added_txids();
        let (event_added, event_removed) = batch.residency_delta();

        batch.commit();

        metrics::counter!(MDELTA_TXS_TOTAL, "direction" => "added").increment(added_count as u64);
        metrics::counter!(MDELTA_TXS_TOTAL, "direction" => "removed")
            .increment(removed_count as u64);
        metrics::counter!(MDELTA_NEW_TXS_TOTAL).increment(outcome.new_txids.len() as u64);

        for txid in outcome.new_txids {
            self.tx_backfill_queue.enqueue(txid);
        }

        self.pubsub
            .publish(
                Subject::MempoolDelta,
                &MempoolDeltaEvent {
                    added: event_added,
                    removed: event_removed,
                },
            )
            .await;

        self.cluster_service
            .sync_clusters_for(&added, &outcome.evicted)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Repos;
    use crate::infra::deps::Deps;
    use futures::StreamExt;

    fn build_inert_reconciler() -> Deps {
        Deps::new(
            Repos::new(testkit::postgres::inert_pool()),
            &testkit::deps::inert_clients(),
        )
    }

    #[tokio::test]
    async fn tick_leaves_the_journal_untouched_when_the_write_fails() {
        let deps = build_inert_reconciler();
        let reconciler = deps.mempool_reconciler();

        deps.mempool_ledger.assert_present(&["a".to_string()]);
        assert_eq!(deps.mempool_ledger.pending_len(), 1);

        let mut backfill_rx = deps
            .tx_backfill_queue
            .take_receiver()
            .expect("receiver taken exactly once");
        // Subscribed before `tick` runs, same reason `get_snapshot_then_delta_stream`
        // subscribes first: a publish that raced a late subscribe would be missed.
        let delta_stream = deps
            .pubsub
            .subscribe::<MempoolDeltaEvent>(Subject::MempoolDelta)
            .await;
        futures::pin_mut!(delta_stream);

        reconciler.tick().await;

        assert_eq!(
            deps.mempool_ledger.pending_len(),
            1,
            "a failed write must not commit the batch: the same prefix has to come back"
        );
        assert!(
            backfill_rx.try_recv().is_err(),
            "nothing should have been enqueued for backfill"
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), delta_stream.next())
                .await
                .is_err(),
            "no MempoolDeltaEvent should have been published"
        );
    }
}
