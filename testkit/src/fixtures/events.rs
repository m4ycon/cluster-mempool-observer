use super::{TX_FEE, TX_VSIZE};
use shared::events::{ClusterRef, MempoolDeltaEvent};

pub struct ClusterRefFixture {
    id: i64,
    txids: Vec<String>,
    total_vsize: i64,
    total_fee: i64,
}

impl ClusterRefFixture {
    pub fn new(id: i64) -> Self {
        Self {
            id,
            txids: vec!["a".into(), "b".into()],
            total_vsize: 2 * TX_VSIZE,
            total_fee: 2 * TX_FEE,
        }
    }

    pub fn with_txids(mut self, txids: &[&str]) -> Self {
        self.txids = txids.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_total_vsize(mut self, total_vsize: i64) -> Self {
        self.total_vsize = total_vsize;
        self
    }

    pub fn with_total_fee(mut self, total_fee: i64) -> Self {
        self.total_fee = total_fee;
        self
    }

    pub fn build(self) -> ClusterRef {
        ClusterRef {
            id: self.id,
            txids: self.txids,
            total_vsize: self.total_vsize,
            total_fee: self.total_fee,
        }
    }
}

/// A poll-to-poll mempool diff.
#[derive(Default)]
pub struct MempoolDeltaEventFixture {
    added: Vec<String>,
    removed: Vec<String>,
}

impl MempoolDeltaEventFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_added(mut self, added: &[&str]) -> Self {
        self.added = added.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_removed(mut self, removed: &[&str]) -> Self {
        self.removed = removed.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn build(self) -> MempoolDeltaEvent {
        MempoolDeltaEvent {
            added: self.added,
            removed: self.removed,
        }
    }
}
