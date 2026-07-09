use crate::services::pubsub::PubSubService;
use futures::Stream;
use shared::events::ClusterDeltaEvent;
use shared::snapshot::ClusterSnapshot;
use shared::subjects::Subject;

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

    pub fn seed(&self, clusters: impl IntoIterator<Item = (i64, Vec<String>, i64, i64)>) {
        self.snapshot.seed(clusters);
    }

    pub async fn stream(&self) -> impl Stream<Item = ClusterDeltaEvent> + use<> {
        self.pubsub
            .subscribe::<ClusterDeltaEvent>(Subject::ClusterDelta)
            .await
    }

    pub fn get_current_snapshot(&self) -> ClusterDeltaEvent {
        self.snapshot.get_current()
    }

    pub async fn publish(
        &self,
        upserted: impl IntoIterator<Item = (i64, Vec<String>, i64, i64)>,
        removed: impl IntoIterator<Item = i64>,
    ) {
        let mut event = ClusterDeltaEvent::default();

        for (id, txids, total_vsize, total_fee) in upserted {
            if let Some(reference) = self.snapshot.upsert(id, txids, total_vsize, total_fee) {
                event.upserted.push(reference);
            }
        }

        for id in removed {
            // only tell clients about clusters they actually knew were active
            if self.snapshot.remove(id) {
                event.removed.push(id);
            }
        }

        if event.upserted.is_empty() && event.removed.is_empty() {
            return;
        }

        self.pubsub.publish(Subject::ClusterDelta, &event).await;
    }
}
