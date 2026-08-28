use crate::services::pubsub::PubSubService;
use futures::Stream;
use shared::events::{ClusterDeltaEvent, ClusterRef};
use shared::metrics::timed;
use shared::snapshot::ClusterSnapshot;
use shared::subjects::Subject;

/// Cluster changes actually broadcast, after diffing against the snapshot.
const CDELTA_PUBLISHED_TOTAL: &str = "cluster_delta_published_total";

/// Active clusters currently tracked in the snapshot.
const CDELTA_ACTIVE_COUNT: &str = "cluster_active_count";

/// Time to clone the whole active set for one newly connected subscriber.
const CDELTA_SNAPSHOT_BUILD_SECONDS: &str = "cluster_snapshot_build_seconds";

/// Clusters per frame in the initial snapshot.
const SNAPSHOT_CHUNK_SIZE: usize = 1_000;

/// Owns the cluster-change streaming concern: the in-memory active-cluster
/// snapshot and the pub/sub bus. Diffs each mutation round against the snapshot
/// and publishes a single `ClusterDeltaEvent`, and serves the initial frame to
/// new subscribers. Cheap to clone (shared `Arc`s).
#[derive(Clone)]
pub struct ClusterDeltaService {
    snapshot: ClusterSnapshot,
    pubsub: PubSubService,
}

impl ClusterDeltaService {
    pub fn new(snapshot: ClusterSnapshot, pubsub: PubSubService) -> Self {
        Self { snapshot, pubsub }
    }

    pub fn seed(&self, clusters: impl IntoIterator<Item = ClusterRef>) {
        self.snapshot.seed(clusters);
    }

    pub async fn stream(&self) -> impl Stream<Item = ClusterDeltaEvent> + use<> {
        self.pubsub
            .subscribe::<ClusterDeltaEvent>(Subject::ClusterDelta)
            .await
    }

    pub fn get_current_snapshot(&self) -> Vec<ClusterDeltaEvent> {
        let mut frames = timed(CDELTA_SNAPSHOT_BUILD_SECONDS, || {
            self.snapshot.get_current_chunked(SNAPSHOT_CHUNK_SIZE)
        });
        if frames.is_empty() {
            frames.push(ClusterDeltaEvent::default());
        }
        frames
    }

    pub fn active_count(&self) -> usize {
        self.snapshot.len()
    }

    pub async fn publish(
        &self,
        upserted: impl IntoIterator<Item = ClusterRef>,
        removed: impl IntoIterator<Item = i64>,
    ) {
        let mut event = ClusterDeltaEvent::default();

        for cluster in upserted {
            if let Some(reference) = self.snapshot.upsert(cluster) {
                event.upserted.push(reference);
            }
        }

        for id in removed {
            // only tell clients about clusters they actually knew were active
            if self.snapshot.remove(id) {
                event.removed.push(id);
            }
        }

        metrics::gauge!(CDELTA_ACTIVE_COUNT).set(self.snapshot.len() as f64);

        if event.upserted.is_empty() && event.removed.is_empty() {
            return;
        }

        metrics::counter!(CDELTA_PUBLISHED_TOTAL, "kind" => "upserted")
            .increment(event.upserted.len() as u64);
        metrics::counter!(CDELTA_PUBLISHED_TOTAL, "kind" => "removed")
            .increment(event.removed.len() as u64);

        self.pubsub.publish(Subject::ClusterDelta, &event).await;
    }
}
