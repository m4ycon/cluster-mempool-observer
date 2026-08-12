use shared::events::MempoolFeerateDiagram;
use shared::snapshot::FeerateDiagramSnapshot;

#[derive(Clone)]
pub struct FeerateDiagramService {
    snapshot: FeerateDiagramSnapshot,
}

impl FeerateDiagramService {
    pub fn new(snapshot: FeerateDiagramSnapshot) -> Self {
        Self { snapshot }
    }

    /// The latest polled diagram, or an unsampled empty one before the first poll.
    pub fn current(&self) -> MempoolFeerateDiagram {
        self.snapshot.get()
    }
}
