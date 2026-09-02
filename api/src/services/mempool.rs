use crate::db::models::{DeltaReason, NewMempoolDelta, NewTransaction};
use crate::db::{MempoolDeltaRepository, TransactionRepository};
use crate::services::cluster::ClusterService;
use crate::services::pubsub::PubSubService;
use crate::services::tx_backfill::TxBackfillQueue;
use futures::{Stream, StreamExt, stream};
use observer::retrievers::{ClusterRetriever, ClusterRpcRetriever};
use shared::events::MempoolDeltaEvent;
use shared::metrics::{timed_async, timed_async_with};
use shared::models::MempoolEntrySummary;
use shared::subjects::Subject;
use std::collections::{HashMap, HashSet};
use std::future::Future;

/// End-to-end time to apply one mempool delta: persist it, queue the
/// transactions it introduced for enrichment, then resync the affected clusters.
const MDELTA_APPLY_SECONDS: &str = "mempool_delta_apply_seconds";

/// Stages of that pipeline which `db_query_seconds` does not already cover
const MDELTA_STAGE_SECONDS: &str = "mempool_delta_stage_seconds";

/// Txids seen entering and leaving the mempool.
const MDELTA_TXS_TOTAL: &str = "mempool_delta_txs_total";

/// Txids that were not already stored, so they were inserted hollow.
const MDELTA_NEW_TXS_TOTAL: &str = "mempool_new_txs_total";

/// Failures at any of the four write steps in `persist_delta_and_txs_inner`,
/// by `stage`. Each one used to be visible only as a log line.
const MEMPOOL_PERSIST_FAILED_TOTAL: &str = "mempool_persist_failed_total";

#[derive(Clone)]
pub struct MempoolService<CR: ClusterRetriever = ClusterRpcRetriever> {
    mempool_delta_repository: MempoolDeltaRepository,
    transaction_repository: TransactionRepository,
    tx_backfill_queue: TxBackfillQueue,
    cluster_service: ClusterService<CR>,
    pubsub: PubSubService,
}

impl<CR: ClusterRetriever> MempoolService<CR> {
    pub fn new(
        mempool_delta_repository: MempoolDeltaRepository,
        transaction_repository: TransactionRepository,
        tx_backfill_queue: TxBackfillQueue,
        cluster_service: ClusterService<CR>,
        pubsub: PubSubService,
    ) -> Self {
        Self {
            mempool_delta_repository,
            transaction_repository,
            tx_backfill_queue,
            cluster_service,
            pubsub,
        }
    }

    pub async fn get_delta_stream(&self) -> impl Stream<Item = MempoolDeltaEvent> + use<CR> {
        self.pubsub
            .subscribe::<MempoolDeltaEvent>(Subject::MempoolDelta)
            .await
    }

    pub async fn get_snapshot_then_delta_stream<F, Fut>(
        &self,
        snapshot_provider: F, // allow us to test some scenarios
    ) -> impl Stream<Item = MempoolDeltaEvent> + use<CR, F, Fut>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = HashSet<String>>,
    {
        // Subscribe first to avoid data gaps
        let delta_stream = self.get_delta_stream().await;

        let snapshot = MempoolDeltaEvent {
            added: snapshot_provider().await.into_iter().collect(),
            removed: Vec::new(),
        };

        stream::once(async move { snapshot }).chain(delta_stream)
    }

    pub async fn persist_deltas_and_new_txs<S>(&self, stream: S)
    where
        S: Stream<Item = MempoolDeltaEvent>,
    {
        let mut stream = std::pin::pin!(stream);
        while let Some(delta) = stream.next().await {
            self.apply_delta(delta).await;
        }
    }

    /// Persists a single mempool delta and queues any newly-seen transactions
    /// for backfill.
    pub async fn apply_delta(&self, delta: MempoolDeltaEvent) {
        // the watcher reports txids only, so each new tx is inserted hollow
        self.persist_delta_and_txs(delta, &HashMap::new()).await;
    }

    /// Bootstrap reconciles against `getrawmempool verbose`, so it can hand over
    /// the entry behind every added txid and store its fee/vsize immediately
    /// instead of inserting it hollow.
    pub async fn apply_bootstrap_delta(
        &self,
        delta: MempoolDeltaEvent,
        entries: HashMap<String, MempoolEntrySummary>,
    ) {
        self.persist_delta_and_txs(delta, &entries).await;
    }

    async fn persist_delta_and_txs(
        &self,
        delta: MempoolDeltaEvent,
        entries: &HashMap<String, MempoolEntrySummary>,
    ) {
        metrics::counter!(MDELTA_TXS_TOTAL, "direction" => "added")
            .increment(delta.added.len() as u64);
        metrics::counter!(MDELTA_TXS_TOTAL, "direction" => "removed")
            .increment(delta.removed.len() as u64);

        timed_async(
            MDELTA_APPLY_SECONDS,
            self.persist_delta_and_txs_inner(delta, entries),
        )
        .await
    }

    async fn persist_delta_and_txs_inner(
        &self,
        delta: MempoolDeltaEvent,
        entries: &HashMap<String, MempoolEntrySummary>,
    ) {
        let added = delta.added.clone();

        let existing_txids = match self.transaction_repository.existing_txids(&added).await {
            Ok(ids) => ids,
            Err(e) => {
                metrics::counter!(MEMPOOL_PERSIST_FAILED_TOTAL, "stage" => "existing_txids")
                    .increment(1);
                tracing::error!("failed to check existing transactions: {e}");
                return;
            }
        };

        let add_rows: Vec<NewMempoolDelta> = added
            .iter()
            .map(|txid| NewMempoolDelta {
                txid: txid.clone(),
                reason: DeltaReason::AddMempool,
            })
            .collect();
        if let Err(e) = self.mempool_delta_repository.insert_many(&add_rows).await {
            metrics::counter!(MEMPOOL_PERSIST_FAILED_TOTAL, "stage" => "insert_deltas")
                .increment(1);
            tracing::error!("failed to persist mempool deltas: {e}");
        }

        let evicted = match self
            .mempool_delta_repository
            .record_removes_for_unpaired(&delta.removed)
            .await
        {
            Ok(evicted) => evicted,
            Err(e) => {
                metrics::counter!(MEMPOOL_PERSIST_FAILED_TOTAL, "stage" => "record_removes")
                    .increment(1);
                tracing::error!("failed to persist mempool removals: {e}");
                Vec::new()
            }
        };

        let new_txids = added
            .clone()
            .into_iter()
            .filter(|txid| !existing_txids.contains(txid))
            .collect::<Vec<_>>();
        metrics::counter!(MDELTA_NEW_TXS_TOTAL).increment(new_txids.len() as u64);

        let insert_new_txs = async {
            let new_rows: Vec<NewTransaction> = new_txids
                .iter()
                .map(|txid| match entries.get(txid) {
                    Some(entry) => NewTransaction::from(entry),
                    None => NewTransaction::hollow(txid),
                })
                .collect();

            match self.transaction_repository.insert_many(&new_rows).await {
                Ok(_) => {
                    for row in &new_rows {
                        if row.needs_backfill() {
                            self.tx_backfill_queue.enqueue(row.txid.clone());
                        }
                    }
                    true
                }
                Err(e) => {
                    metrics::counter!(MEMPOOL_PERSIST_FAILED_TOTAL, "stage" => "insert_new_txs")
                        .increment(1);
                    tracing::error!("failed to persist transactions: {e}");
                    false
                }
            }
        };
        let insert_succeeded = timed_async_with(
            MDELTA_STAGE_SECONDS,
            &[("stage", "insert_new_txs")],
            insert_new_txs,
        )
        .await;

        // apply cluster evictions plus new-tx clusters
        timed_async_with(
            MDELTA_STAGE_SECONDS,
            &[("stage", "sync_clusters")],
            self.cluster_service.sync_clusters_for(
                cluster_sync_candidates(insert_succeeded, &added, &existing_txids),
                &evicted,
            ),
        )
        .await;
    }
}

/// Txids cluster sync may be driven on: all of `added` once the write lands,
/// or only `existing_txids` when it fails, since nothing new landed then.
///
/// This picks which txids we ask the node about, nothing more. Membership
/// comes from the node's answer, so a surviving candidate can still drag a
/// missing cluster-mate into a cluster row; the guard is total only when
/// nothing survives. Closing that gap needs the unknown members persisted.
fn cluster_sync_candidates<'a>(
    insert_succeeded: bool,
    added: &'a [String],
    existing_txids: &'a [String],
) -> &'a [String] {
    if insert_succeeded {
        added
    } else {
        existing_txids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Repos;
    use crate::infra::deps::Deps;
    use crate::services::pubsub::PubSubService;
    use std::time::Duration;

    fn build_service() -> (MempoolService, PubSubService) {
        let deps = Deps::new(
            Repos::new(testkit::postgres::inert_pool()),
            &testkit::deps::inert_clients(),
        );
        (deps.mempool_service(), deps.pubsub)
    }

    /// Drives `get_snapshot_then_delta_stream` with `initial` as the snapshot and
    /// `delta` published mid-setup, then folds snapshot+delta the way a client
    /// would and asserts the resulting set equals `expected`.
    ///
    /// The second read is bounded so a swallowed delta fails instead of hanging.
    async fn assert_get_snapshot_then_delta_stream_final_set(
        delta: MempoolDeltaEvent,
        initial: &[&str],
        expected: &[&str],
    ) {
        let (service, pubsub) = build_service();
        let snapshot: HashSet<String> = initial.iter().map(|s| s.to_string()).collect();

        let snapshot_provider = || async move {
            // publishing delta while the snapshot is being read
            pubsub.publish(Subject::MempoolDelta, &delta).await;
            snapshot
        };

        let stream = service
            .get_snapshot_then_delta_stream(snapshot_provider)
            .await;
        futures::pin_mut!(stream);

        // snapshot
        let snapshot_frame = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("snapshot frame not received")
            .expect("snapshot frame");
        let mut set: HashSet<String> = snapshot_frame.added.into_iter().collect();

        // the delta (must be delivered even when redundant / a no-op)
        let delta_frame = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("delta frame not received: delta was lost during ws setup")
            .expect("delta frame");
        set.extend(delta_frame.added);
        for txid in delta_frame.removed {
            set.remove(&txid);
        }

        let mut got: Vec<String> = set.into_iter().collect();
        got.sort();
        let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(got, want);
    }

    /// snapshot {a, b}; a delta adds c during setup => final set {a, b, c}
    #[tokio::test]
    async fn get_snapshot_then_delta_stream_applies_added_txid() {
        assert_get_snapshot_then_delta_stream_final_set(
            MempoolDeltaEvent {
                added: vec!["c".to_string()],
                removed: Vec::new(),
            },
            &["a", "b"],
            &["a", "b", "c"],
        )
        .await;
    }

    /// snapshot {a, b, c}; a delta removes c during setup => final set {a, b}
    #[tokio::test]
    async fn get_snapshot_then_delta_stream_applies_removed_txid() {
        assert_get_snapshot_then_delta_stream_final_set(
            MempoolDeltaEvent {
                added: Vec::new(),
                removed: vec!["c".to_string()],
            },
            &["a", "b", "c"],
            &["a", "b"],
        )
        .await;
    }

    /// snapshot {a, b}; a delta re-adds b during setup => final set still {a, b}
    #[tokio::test]
    async fn get_snapshot_then_delta_stream_ignores_redundant_add() {
        assert_get_snapshot_then_delta_stream_final_set(
            MempoolDeltaEvent {
                added: vec!["b".to_string()],
                removed: Vec::new(),
            },
            &["a", "b"],
            &["a", "b"],
        )
        .await;
    }

    /// snapshot {a, b}; a delta removes c (absent) during setup => final set still {a, b}
    #[tokio::test]
    async fn get_snapshot_then_delta_stream_ignores_absent_remove() {
        assert_get_snapshot_then_delta_stream_final_set(
            MempoolDeltaEvent {
                added: Vec::new(),
                removed: vec!["c".to_string()],
            },
            &["a", "b"],
            &["a", "b"],
        )
        .await;
    }

    #[test]
    fn cluster_sync_candidates_is_the_full_add_list_when_the_insert_lands() {
        let added = vec!["a".to_string(), "b".to_string()];
        let existing = vec!["a".to_string()];
        assert_eq!(cluster_sync_candidates(true, &added, &existing), &added[..]);
    }

    #[test]
    fn cluster_sync_candidates_falls_back_to_existing_txids_when_the_insert_fails() {
        let added = vec!["a".to_string(), "b".to_string()];
        let existing = vec!["a".to_string()];
        assert_eq!(
            cluster_sync_candidates(false, &added, &existing),
            &existing[..]
        );
    }
}
