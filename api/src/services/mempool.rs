use crate::db::models::{DeltaReason, NewMempoolDelta, NewTransaction};
use crate::db::{MempoolDeltaRepository, TransactionRepository};
use crate::services::cluster::ClusterService;
use crate::services::pubsub::PubSubService;
use futures::{Stream, StreamExt, stream};
use observer::retrievers::{
    ClusterRetriever, ClusterRpcRetriever, TransactionRetriever, TransactionRpcRetriever,
};
use shared::events::MempoolDeltaEvent;
use shared::metrics::{timed_async, timed_async_with};
use shared::subjects::Subject;
use std::collections::{HashMap, HashSet};
use std::future::Future;

const MAX_CONCURRENT_TXS_INSERTS: usize = 4;

/// End-to-end time to apply one mempool delta: persist it, backfill the
/// transactions it introduced, then resync the affected clusters.
const MDELTA_APPLY_SECONDS: &str = "mempool_delta_apply_seconds";

/// Stages of that pipeline which `db_query_seconds` does not already cover
const MDELTA_STAGE_SECONDS: &str = "mempool_delta_stage_seconds";

/// Txids seen entering and leaving the mempool.
const MDELTA_TXS_TOTAL: &str = "mempool_delta_txs_total";

/// Txids that were not already stored and had to be fetched from the node.
const MDELTA_NEW_TXS_TOTAL: &str = "mempool_new_txs_total";

#[derive(Clone)]
pub struct MempoolService<
    TR: TransactionRetriever = TransactionRpcRetriever,
    CR: ClusterRetriever = ClusterRpcRetriever,
> {
    mempool_delta_repository: MempoolDeltaRepository,
    transaction_repository: TransactionRepository,
    transaction_retriever: TR,
    cluster_service: ClusterService<CR>,
    pubsub: PubSubService,
}

impl<TR: TransactionRetriever, CR: ClusterRetriever> MempoolService<TR, CR> {
    pub fn new(
        mempool_delta_repository: MempoolDeltaRepository,
        transaction_repository: TransactionRepository,
        transaction_retriever: TR,
        cluster_service: ClusterService<CR>,
        pubsub: PubSubService,
    ) -> Self {
        Self {
            mempool_delta_repository,
            transaction_repository,
            transaction_retriever,
            cluster_service,
            pubsub,
        }
    }

    pub async fn get_delta_stream(&self) -> impl Stream<Item = MempoolDeltaEvent> + use<TR, CR> {
        self.pubsub
            .subscribe::<MempoolDeltaEvent>(Subject::MempoolDelta)
            .await
    }

    pub async fn get_snapshot_then_delta_stream<F, Fut>(
        &self,
        snapshot_provider: F, // allow us to test some scenarios
    ) -> impl Stream<Item = MempoolDeltaEvent> + use<TR, CR, F, Fut>
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

    /// Persists a single mempool delta and backfills any newly-seen transactions.
    pub async fn apply_delta(&self, delta: MempoolDeltaEvent) {
        // TODO: fee not available from getrawtransaction non-verbose
        self.persist_delta_and_txs(delta, &HashMap::new()).await;
    }

    pub async fn apply_bootstrap_delta(
        &self,
        delta: MempoolDeltaEvent,
        fees: HashMap<String, i64>,
    ) {
        self.persist_delta_and_txs(delta, &fees).await;
    }

    async fn persist_delta_and_txs(&self, delta: MempoolDeltaEvent, fees: &HashMap<String, i64>) {
        metrics::counter!(MDELTA_TXS_TOTAL, "direction" => "added")
            .increment(delta.added.len() as u64);
        metrics::counter!(MDELTA_TXS_TOTAL, "direction" => "removed")
            .increment(delta.removed.len() as u64);

        timed_async(
            MDELTA_APPLY_SECONDS,
            self.persist_delta_and_txs_inner(delta, fees),
        )
        .await
    }

    async fn persist_delta_and_txs_inner(
        &self,
        delta: MempoolDeltaEvent,
        fees: &HashMap<String, i64>,
    ) {
        let added = delta.added.clone();

        let existing_txids = match self.transaction_repository.existing_txids(&added).await {
            Ok(ids) => ids,
            Err(e) => {
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
            tracing::error!("failed to persist mempool deltas: {e}");
        }

        let evicted = match self
            .mempool_delta_repository
            .record_removes_for_unpaired(&delta.removed)
            .await
        {
            Ok(evicted) => evicted,
            Err(e) => {
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

        // fetch and persist new transactions concurrently
        let fetch_new_txs = stream::iter(new_txids)
            .map(async |txid| {
                let client = self.transaction_retriever.clone();
                let mut new_tx = match client.get_raw_transaction(&txid).await {
                    Ok(tx) => NewTransaction::from(&tx),
                    Err(e) => {
                        tracing::error!("failed to retrieve transaction, persisting hollow: {e:?}");
                        NewTransaction::hollow(&txid)
                    }
                };
                new_tx.fee = fees.get(&txid).copied();

                if let Err(e) = self.transaction_repository.insert(&new_tx).await {
                    tracing::error!("failed to persist transaction: {e}");
                }
            })
            .buffer_unordered(MAX_CONCURRENT_TXS_INSERTS)
            .collect::<Vec<_>>();
        timed_async_with(
            MDELTA_STAGE_SECONDS,
            &[("stage", "fetch_new_txs")],
            fetch_new_txs,
        )
        .await;

        // apply cluster evictions plus new-tx clusters
        timed_async_with(
            MDELTA_STAGE_SECONDS,
            &[("stage", "sync_clusters")],
            self.cluster_service.sync_clusters_for(&added, &evicted),
        )
        .await;
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
}
