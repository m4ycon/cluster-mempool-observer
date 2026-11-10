use crate::api::FeerateDiagramComparison;
use std::sync::{Arc, RwLock};

/// The last comparison the gauge sampler computed, shared with the routes.
#[derive(Clone, Default)]
pub struct FeerateComparisonSnapshot {
    inner: Arc<RwLock<Option<FeerateDiagramComparison>>>,
}

impl FeerateComparisonSnapshot {
    pub fn store(&self, comparison: FeerateDiagramComparison) {
        *self
            .inner
            .write()
            .expect("feerate comparison snapshot poisoned") = Some(comparison);
    }

    pub fn get(&self) -> Option<FeerateDiagramComparison> {
        self.inner
            .read()
            .expect("feerate comparison snapshot poisoned")
            .clone()
    }
}
