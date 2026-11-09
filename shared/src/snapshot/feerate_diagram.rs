use crate::api::MempoolFeerateDiagram;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use time::OffsetDateTime;

const RETAINED_DIAGRAMS: usize = 6;

/// Live, shareable snapshot of the latest `getmempoolfeeratediagram` polls.
#[derive(Clone, Default)]
pub struct FeerateDiagramSnapshot {
    /// Oldest first.
    inner: Arc<RwLock<VecDeque<MempoolFeerateDiagram>>>,
}

impl FeerateDiagramSnapshot {
    pub fn store(&self, diagram: MempoolFeerateDiagram) {
        let mut diagrams = self
            .inner
            .write()
            .expect("feerate diagram snapshot poisoned");
        if diagrams.len() == RETAINED_DIAGRAMS {
            diagrams.pop_front();
        }
        diagrams.push_back(diagram);
    }

    pub fn get(&self) -> MempoolFeerateDiagram {
        self.inner
            .read()
            .expect("feerate diagram snapshot poisoned")
            .back()
            .cloned()
            .unwrap_or_default()
    }

    pub fn latest_at_or_before(&self, instant: OffsetDateTime) -> Option<MempoolFeerateDiagram> {
        self.inner
            .read()
            .expect("feerate diagram snapshot poisoned")
            .iter()
            .rev()
            .find(|diagram| diagram.sampled_at.is_some_and(|at| at <= instant))
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.inner
            .read()
            .expect("feerate diagram snapshot poisoned")
            .back()
            .map_or(0, |diagram| diagram.points.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::FeerateDiagramPoint;
    use time::Duration;

    const T0: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

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

    fn get_diagram_sampled_after(secs_after_t0: i64) -> MempoolFeerateDiagram {
        diagram(Some(T0 + Duration::seconds(secs_after_t0)), vec![])
    }

    fn sampled_at_of(diagram: Option<MempoolFeerateDiagram>) -> Option<OffsetDateTime> {
        diagram.and_then(|diagram| diagram.sampled_at)
    }

    #[test]
    fn get_returns_the_most_recently_stored_diagram() {
        let snap = FeerateDiagramSnapshot::default();
        snap.store(get_diagram_sampled_after(0));
        snap.store(get_diagram_sampled_after(10));

        assert_eq!(snap.get().sampled_at, Some(T0 + Duration::seconds(10)));
    }

    #[test]
    fn latest_at_or_before_picks_the_newest_diagram_old_enough() {
        let snap = FeerateDiagramSnapshot::default();
        snap.store(get_diagram_sampled_after(0));
        snap.store(get_diagram_sampled_after(10));
        snap.store(get_diagram_sampled_after(20));

        let got = snap.latest_at_or_before(T0 + Duration::seconds(15));

        assert_eq!(sampled_at_of(got), Some(T0 + Duration::seconds(10)));
    }

    #[test]
    fn latest_at_or_before_includes_a_diagram_sampled_exactly_at_the_instant() {
        let snap = FeerateDiagramSnapshot::default();
        snap.store(get_diagram_sampled_after(10));

        let got = snap.latest_at_or_before(T0 + Duration::seconds(10));

        assert_eq!(sampled_at_of(got), Some(T0 + Duration::seconds(10)));
    }

    #[test]
    fn latest_at_or_before_finds_nothing_when_every_diagram_is_too_recent() {
        let snap = FeerateDiagramSnapshot::default();
        snap.store(get_diagram_sampled_after(10));

        assert!(snap.latest_at_or_before(T0).is_none());
    }

    #[test]
    fn latest_at_or_before_finds_nothing_in_an_empty_snapshot() {
        let snap = FeerateDiagramSnapshot::default();

        assert!(snap.latest_at_or_before(T0).is_none());
    }

    #[test]
    fn latest_at_or_before_skips_a_diagram_with_no_sample_time() {
        let snap = FeerateDiagramSnapshot::default();
        snap.store(get_diagram_sampled_after(0));
        snap.store(diagram(None, vec![]));

        let got = snap.latest_at_or_before(T0 + Duration::seconds(10));

        assert_eq!(sampled_at_of(got), Some(T0));
    }

    #[test]
    fn store_drops_the_oldest_diagram_past_the_retention_limit() {
        let snap = FeerateDiagramSnapshot::default();
        for i in 0..=RETAINED_DIAGRAMS as i64 {
            snap.store(get_diagram_sampled_after(i));
        }

        assert!(snap.latest_at_or_before(T0).is_none());
        let oldest_kept = snap.latest_at_or_before(T0 + Duration::seconds(1));
        assert_eq!(sampled_at_of(oldest_kept), Some(T0 + Duration::seconds(1)));
    }
}
