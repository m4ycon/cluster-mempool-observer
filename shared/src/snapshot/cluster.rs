use crate::events::{ClusterDeltaEvent, ClusterRef};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use time::OffsetDateTime;

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
    first_seen_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClusterStats {
    pub cluster_count: usize,
    pub tx_count: usize,
    pub total_vsize: i64,
    pub total_fee: i64,
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
                        first_seen_at: c.first_seen_at,
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
            first_seen_at: cluster.first_seen_at,
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

    /// Number of active clusters currently tracked.
    pub fn len(&self) -> usize {
        self.inner.read().expect("cluster snapshot poisoned").len()
    }

    /// Whether no active clusters are tracked.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cluster totals in one lock pass; no cloning.
    pub fn stats(&self) -> ClusterStats {
        let map = self.inner.read().expect("cluster snapshot poisoned");
        let mut stats = ClusterStats {
            cluster_count: map.len(),
            ..Default::default()
        };
        for state in map.values() {
            stats.tx_count += state.txids.len();
            stats.total_vsize += state.total_vsize;
            stats.total_fee += state.total_fee;
        }
        stats
    }

    /// The full active set as initial `upserted`-only change events, at most
    /// `chunk_size` clusters each.
    pub fn get_current_chunked(&self, chunk_size: usize) -> Vec<ClusterDeltaEvent> {
        let chunk_size = chunk_size.max(1);
        let map = self.inner.read().expect("cluster snapshot poisoned");

        let mut events = Vec::with_capacity(map.len().div_ceil(chunk_size));
        let mut upserted = Vec::with_capacity(chunk_size.min(map.len()));
        for (id, state) in map.iter() {
            upserted.push(ClusterRef {
                id: *id,
                txids: state.txids.clone(),
                total_vsize: state.total_vsize,
                total_fee: state.total_fee,
                first_seen_at: state.first_seen_at,
            });
            if upserted.len() == chunk_size {
                events.push(ClusterDeltaEvent {
                    upserted: std::mem::take(&mut upserted),
                    removed: Vec::new(),
                });
            }
        }
        if !upserted.is_empty() {
            events.push(ClusterDeltaEvent {
                upserted,
                removed: Vec::new(),
            });
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testkit::fixtures::fixed_time;

    fn txids(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| s.to_string()).collect()
    }

    fn current(snap: &ClusterSnapshot) -> ClusterDeltaEvent {
        let mut chunks = snap.get_current_chunked(usize::MAX);
        assert!(chunks.len() <= 1);
        chunks.pop().unwrap_or_default()
    }

    fn cluster(id: i64, txids: Vec<String>, total_vsize: i64, total_fee: i64) -> ClusterRef {
        ClusterRef {
            id,
            txids,
            total_vsize,
            total_fee,
            first_seen_at: fixed_time(),
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
    fn len_tracks_the_active_cluster_count() {
        let snap = ClusterSnapshot::default();
        assert_eq!(snap.len(), 0);
        assert!(snap.is_empty());

        snap.upsert(cluster(1, txids(&["a"]), 10, 20));
        snap.upsert(cluster(2, txids(&["b"]), 10, 20));
        assert_eq!(snap.len(), 2);
        assert!(!snap.is_empty());

        snap.remove(1);
        assert_eq!(snap.len(), 1);
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
        let active = current(&snap);
        assert_eq!(active.upserted.len(), 2);
        assert!(active.removed.is_empty());

        snap.seed([cluster(9, txids(&["z"]), 90, 900)]);
        let active = current(&snap);
        assert_eq!(active.upserted.len(), 1);
        assert_eq!(active.upserted[0].id, 9);
    }

    #[test]
    fn first_seen_at_survives_upsert_and_seed() {
        let seen = OffsetDateTime::UNIX_EPOCH + time::Duration::days(1);
        let snap = ClusterSnapshot::default();

        let mut fresh = cluster(1, txids(&["a"]), 50, 100);
        fresh.first_seen_at = seen;
        assert_eq!(snap.upsert(fresh).unwrap().first_seen_at, seen);
        assert_eq!(current(&snap).upserted[0].first_seen_at, seen);

        let mut seeded = cluster(9, txids(&["z"]), 90, 900);
        seeded.first_seen_at = seen;
        snap.seed([seeded]);
        assert_eq!(current(&snap).upserted[0].first_seen_at, seen);
    }

    #[test]
    fn get_current_chunked_splits_the_set_without_losing_clusters() {
        let snap = ClusterSnapshot::default();
        for id in 1..=5i64 {
            snap.upsert(cluster(id, txids(&["a"]), 50, 100));
        }

        let chunks = snap.get_current_chunked(2);
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|c| c.upserted.len() <= 2));
        assert!(chunks.iter().all(|c| c.removed.is_empty()));

        let mut ids: Vec<i64> = chunks
            .iter()
            .flat_map(|c| c.upserted.iter().map(|r| r.id))
            .collect();
        ids.sort();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn get_current_chunked_on_empty_snapshot_yields_no_frames() {
        assert!(ClusterSnapshot::default().get_current_chunked(2).is_empty());
    }

    #[test]
    fn stats_on_empty_snapshot_is_zero() {
        let snap = ClusterSnapshot::default();
        assert_eq!(snap.stats(), ClusterStats::default());
    }

    #[test]
    fn stats_sums_across_clusters() {
        let snap = ClusterSnapshot::default();
        snap.upsert(cluster(1, txids(&["a", "b"]), 50, 100));
        snap.upsert(cluster(2, txids(&["c", "d", "e"]), 30, 60));

        let stats = snap.stats();
        assert_eq!(stats.cluster_count, 2);
        assert_eq!(stats.tx_count, 5);
        assert_eq!(stats.total_vsize, 80);
        assert_eq!(stats.total_fee, 160);
    }

    #[test]
    fn stats_reflects_upsert_remove_and_seed() {
        let snap = ClusterSnapshot::default();
        snap.upsert(cluster(1, txids(&["a"]), 10, 20));
        snap.upsert(cluster(2, txids(&["b", "c"]), 15, 25));
        assert_eq!(snap.stats().cluster_count, 2);

        snap.remove(1);
        let stats = snap.stats();
        assert_eq!(
            stats,
            ClusterStats {
                cluster_count: 1,
                tx_count: 2,
                total_vsize: 15,
                total_fee: 25,
            }
        );

        snap.seed([cluster(9, txids(&["z", "y", "x"]), 90, 900)]);
        let stats = snap.stats();
        assert_eq!(
            stats,
            ClusterStats {
                cluster_count: 1,
                tx_count: 3,
                total_vsize: 90,
                total_fee: 900,
            }
        );
    }
}
