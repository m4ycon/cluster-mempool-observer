use crate::services::pubsub::PubSubService;
use futures::Stream;
use shared::events::{ClusterDeltaEvent, ClusterRef};
use shared::subjects::Subject;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

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

        for (id, txids, total_size, total_fee) in upserted {
            if let Some(reference) = self.snapshot.upsert(id, txids, total_size, total_fee) {
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

/// In-memory view of the current active (unconfirmed, non-empty) cluster set,
/// keyed by cluster id. Used to diff each cluster mutation round and to seed the
/// initial frame sent to new websocket subscribers.
#[derive(Clone, Default)]
pub struct ClusterSnapshot {
    inner: Arc<RwLock<HashMap<i64, ClusterState>>>,
}

#[derive(Clone, PartialEq)]
struct ClusterState {
    txids: Vec<String>,
    total_size: i64,
    total_fee: i64,
}

impl ClusterSnapshot {
    fn seed(&self, clusters: impl IntoIterator<Item = (i64, Vec<String>, i64, i64)>) {
        let map = clusters
            .into_iter()
            .map(|(id, txids, total_size, total_fee)| {
                (
                    id,
                    ClusterState {
                        txids,
                        total_size,
                        total_fee,
                    },
                )
            })
            .collect();
        *self.inner.write().expect("cluster snapshot poisoned") = map;
    }

    /// Records an upserted cluster. Returns `Some(ClusterRef)` if it is new or
    /// changed (membership or fee), updating the tracked state; `None` if it
    /// matches what we already had.
    fn upsert(
        &self,
        id: i64,
        txids: Vec<String>,
        total_size: i64,
        total_fee: i64,
    ) -> Option<ClusterRef> {
        let state = ClusterState {
            txids,
            total_size,
            total_fee,
        };
        let mut map = self.inner.write().expect("cluster snapshot poisoned");
        match map.get(&id) {
            Some(prev) if *prev == state => None,
            _ => {
                map.insert(id, state.clone());
                Some(ClusterRef {
                    id,
                    txids: state.txids,
                    total_size: state.total_size,
                    total_fee: state.total_fee,
                })
            }
        }
    }

    /// Drops a cluster from the active set. Returns `true` if it was tracked
    /// (i.e. the removal is worth telling clients about).
    fn remove(&self, id: i64) -> bool {
        self.inner
            .write()
            .expect("cluster snapshot poisoned")
            .remove(&id)
            .is_some()
    }

    /// The full active set as an initial `upserted`-only change event.
    fn get_current(&self) -> ClusterDeltaEvent {
        let map = self.inner.read().expect("cluster snapshot poisoned");
        ClusterDeltaEvent {
            upserted: map
                .iter()
                .map(|(id, s)| ClusterRef {
                    id: *id,
                    txids: s.txids.clone(),
                    total_size: s.total_size,
                    total_fee: s.total_fee,
                })
                .collect(),
            removed: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txids(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn upsert_reports_new_cluster() {
        let snap = ClusterSnapshot::default();
        let reference = snap.upsert(1, txids(&["a", "b"]), 50, 100).expect("new");
        assert_eq!(reference.id, 1);
        assert_eq!(reference.txids, txids(&["a", "b"]));
        assert_eq!(reference.total_size, 50);
        assert_eq!(reference.total_fee, 100);
    }

    #[test]
    fn upsert_is_silent_when_unchanged() {
        let snap = ClusterSnapshot::default();
        snap.upsert(1, txids(&["a", "b"]), 50, 100);
        assert!(snap.upsert(1, txids(&["a", "b"]), 50, 100).is_none());
    }

    #[test]
    fn upsert_reports_membership_and_fee_changes() {
        let snap = ClusterSnapshot::default();
        snap.upsert(1, txids(&["a", "b"]), 50, 100);
        assert!(snap.upsert(1, txids(&["a", "b", "c"]), 50, 100).is_some()); // grew
        assert!(snap.upsert(1, txids(&["a", "b", "c"]), 70, 100).is_some()); // size changed
        assert!(snap.upsert(1, txids(&["a", "b", "c"]), 70, 150).is_some()); // fee changed
        assert!(snap.upsert(1, txids(&["a", "b", "c"]), 70, 150).is_none()); // stable again
    }

    #[test]
    fn remove_reports_only_tracked_clusters() {
        let snap = ClusterSnapshot::default();
        snap.upsert(1, txids(&["a"]), 50, 100);
        assert!(snap.remove(1));
        assert!(!snap.remove(1)); // already gone
        assert!(!snap.remove(2)); // never tracked
    }

    #[test]
    fn get_active_returns_full_set_then_seed_replaces() {
        let snap = ClusterSnapshot::default();
        snap.upsert(1, txids(&["a"]), 50, 100);
        snap.upsert(2, txids(&["b"]), 60, 200);
        let active = snap.get_current();
        assert_eq!(active.upserted.len(), 2);
        assert!(active.removed.is_empty());

        snap.seed([(9, txids(&["z"]), 90, 900)]);
        let active = snap.get_current();
        assert_eq!(active.upserted.len(), 1);
        assert_eq!(active.upserted[0].id, 9);
    }
}
