use crate::services::pubsub::PubSubService;
use futures::{Stream, StreamExt, stream};
use shared::events::MempoolDeltaEvent;
use shared::subjects::Subject;
use std::collections::HashSet;
use std::future::Future;

#[derive(Clone)]
pub struct MempoolService {
    pubsub: PubSubService,
}

impl MempoolService {
    pub fn new(pubsub: PubSubService) -> Self {
        Self { pubsub }
    }

    pub async fn get_delta_stream(&self) -> impl Stream<Item = MempoolDeltaEvent> + use<> {
        self.pubsub
            .subscribe::<MempoolDeltaEvent>(Subject::MempoolDelta)
            .await
    }

    pub async fn get_snapshot_then_delta_stream<F, Fut>(
        &self,
        snapshot_provider: F, // allow us to test some scenarios
    ) -> impl Stream<Item = MempoolDeltaEvent> + use<F, Fut>
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Repos;
    use crate::infra::deps::Deps;
    use crate::services::pubsub::PubSubService;
    use futures::StreamExt;
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
