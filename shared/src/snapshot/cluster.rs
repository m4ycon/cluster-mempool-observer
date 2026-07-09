use crate::events::{ClusterDeltaEvent, ClusterRef};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

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
    total_vsize: i64,
    total_fee: i64,
}

impl ClusterSnapshot {
    pub fn seed(&self, clusters: impl IntoIterator<Item = ClusterRef>) {
        let map = clusters
            .into_iter()
            .map(|c| {
                (
                    c.id,
                    ClusterState {
                        txids: c.txids,
                        total_vsize: c.total_vsize,
                        total_fee: c.total_fee,
                    },
                )
            })
            .collect();
        *self.inner.write().expect("cluster snapshot poisoned") = map;
    }

    /// Records an upserted cluster. Returns `Some(ClusterRef)` if it is new or
    /// changed (membership or fee), updating the tracked state; `None` if it
    /// matches what we already had.
    pub fn upsert(&self, cluster: ClusterRef) -> Option<ClusterRef> {
        let state = ClusterState {
            txids: cluster.txids.clone(),
            total_vsize: cluster.total_vsize,
            total_fee: cluster.total_fee,
        };
        let mut map = self.inner.write().expect("cluster snapshot poisoned");
        match map.get(&cluster.id) {
            Some(prev) if *prev == state => None,
            _ => {
                map.insert(cluster.id, state);
                Some(cluster)
            }
        }
    }

    /// Drops a cluster from the active set. Returns `true` if it was tracked
    /// (i.e. the removal is worth telling clients about).
    pub fn remove(&self, id: i64) -> bool {
        self.inner
            .write()
            .expect("cluster snapshot poisoned")
            .remove(&id)
            .is_some()
    }

    /// The full active set as an initial `upserted`-only change event.
    pub fn get_current(&self) -> ClusterDeltaEvent {
        let map = self.inner.read().expect("cluster snapshot poisoned");
        ClusterDeltaEvent {
            upserted: map
                .iter()
                .map(|(id, s)| ClusterRef {
                    id: *id,
                    txids: s.txids.clone(),
                    total_vsize: s.total_vsize,
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

    fn cluster(id: i64, txids: Vec<String>, total_vsize: i64, total_fee: i64) -> ClusterRef {
        ClusterRef {
            id,
            txids,
            total_vsize,
            total_fee,
        }
    }

    #[test]
    fn upsert_reports_new_cluster() {
        let snap = ClusterSnapshot::default();
        let reference = snap
            .upsert(cluster(1, txids(&["a", "b"]), 50, 100))
            .expect("new");
        assert_eq!(reference.id, 1);
        assert_eq!(reference.txids, txids(&["a", "b"]));
        assert_eq!(reference.total_vsize, 50);
        assert_eq!(reference.total_fee, 100);
    }

    #[test]
    fn upsert_is_silent_when_unchanged() {
        let snap = ClusterSnapshot::default();
        snap.upsert(cluster(1, txids(&["a", "b"]), 50, 100));
        assert!(
            snap.upsert(cluster(1, txids(&["a", "b"]), 50, 100))
                .is_none()
        );
    }

    #[test]
    fn upsert_reports_membership_and_fee_changes() {
        let snap = ClusterSnapshot::default();
        snap.upsert(cluster(1, txids(&["a", "b"]), 50, 100));
        assert!(
            snap.upsert(cluster(1, txids(&["a", "b", "c"]), 50, 100))
                .is_some()
        ); // grew
        assert!(
            snap.upsert(cluster(1, txids(&["a", "b", "c"]), 70, 100))
                .is_some()
        ); // size changed
        assert!(
            snap.upsert(cluster(1, txids(&["a", "b", "c"]), 70, 150))
                .is_some()
        ); // fee changed
        assert!(
            snap.upsert(cluster(1, txids(&["a", "b", "c"]), 70, 150))
                .is_none()
        ); // stable again
    }

    #[test]
    fn remove_reports_only_tracked_clusters() {
        let snap = ClusterSnapshot::default();
        snap.upsert(cluster(1, txids(&["a"]), 50, 100));
        assert!(snap.remove(1));
        assert!(!snap.remove(1)); // already gone
        assert!(!snap.remove(2)); // never tracked
    }

    #[test]
    fn get_active_returns_full_set_then_seed_replaces() {
        let snap = ClusterSnapshot::default();
        snap.upsert(cluster(1, txids(&["a"]), 50, 100));
        snap.upsert(cluster(2, txids(&["b"]), 60, 200));
        let active = snap.get_current();
        assert_eq!(active.upserted.len(), 2);
        assert!(active.removed.is_empty());

        snap.seed([cluster(9, txids(&["z"]), 90, 900)]);
        let active = snap.get_current();
        assert_eq!(active.upserted.len(), 1);
        assert_eq!(active.upserted[0].id, 9);
    }
}
