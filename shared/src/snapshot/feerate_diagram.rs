use crate::events::MempoolFeerateDiagram;
use std::sync::{Arc, RwLock};

/// Live, shareable snapshot of the latest `getmempoolfeeratediagram` poll.
#[derive(Clone, Default)]
pub struct FeerateDiagramSnapshot {
    inner: Arc<RwLock<MempoolFeerateDiagram>>,
}

impl FeerateDiagramSnapshot {
    pub fn store(&self, diagram: MempoolFeerateDiagram) {
        *self
            .inner
            .write()
            .expect("feerate diagram snapshot poisoned") = diagram;
    }

    pub fn get(&self) -> MempoolFeerateDiagram {
        self.inner
            .read()
            .expect("feerate diagram snapshot poisoned")
            .clone()
    }

    pub fn len(&self) -> usize {
        self.inner
            .read()
            .expect("feerate diagram snapshot poisoned")
            .points
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::FeerateDiagramPoint;
    use time::OffsetDateTime;

    fn diagram(
        sampled_at: Option<OffsetDateTime>,
        points: Vec<(u64, i64)>,
    ) -> MempoolFeerateDiagram {
        MempoolFeerateDiagram {
            sampled_at,
            points: points
                .into_iter()
                .map(|(weight, fee_sats)| FeerateDiagramPoint { weight, fee_sats })
                .collect(),
        }
    }

    #[test]
    fn len_and_is_empty_track_store() {
        let snap = FeerateDiagramSnapshot::default();
        assert_eq!(snap.len(), 0);
        assert!(snap.is_empty());

        snap.store(diagram(None, vec![(0, 0), (441, 2712)]));
        assert_eq!(snap.len(), 2);
        assert!(!snap.is_empty());

        snap.store(diagram(None, vec![]));
        assert_eq!(snap.len(), 0);
        assert!(snap.is_empty());
    }

    #[test]
    fn get_round_trips_store() {
        let snap = FeerateDiagramSnapshot::default();
        let sampled_at = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        snap.store(diagram(Some(sampled_at), vec![(441, 2712)]));

        let got = snap.get();
        assert_eq!(got.sampled_at, Some(sampled_at));
        assert_eq!(got.points.len(), 1);
        assert_eq!(got.points[0].weight, 441);
        assert_eq!(got.points[0].fee_sats, 2712);
    }

    #[test]
    fn default_snapshot_is_empty() {
        let snap = FeerateDiagramSnapshot::default();
        let got = snap.get();
        assert_eq!(got.sampled_at, None);
        assert!(got.points.is_empty());
    }
}
