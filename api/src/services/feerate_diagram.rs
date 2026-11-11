use shared::api::{FeerateDiagramComparison, MempoolFeerateDiagram};
use shared::snapshot::{FeerateComparisonSnapshot, FeerateDiagramSnapshot};

#[derive(Clone)]
pub struct FeerateDiagramService {
    snapshot: FeerateDiagramSnapshot,
    comparison_snapshot: FeerateComparisonSnapshot,
}

impl FeerateDiagramService {
    pub fn new(
        snapshot: FeerateDiagramSnapshot,
        comparison_snapshot: FeerateComparisonSnapshot,
    ) -> Self {
        Self {
            snapshot,
            comparison_snapshot,
        }
    }

    /// The latest polled diagram, or an unsampled empty one before the first poll.
    pub fn current(&self) -> MempoolFeerateDiagram {
        self.snapshot.get()
    }

    /// The gauge sampler's last comparison, `None` until its first reading.
    pub fn comparison(&self) -> Option<FeerateDiagramComparison> {
        self.comparison_snapshot.get()
    }
}
