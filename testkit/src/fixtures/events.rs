use super::{TX_FEE, TX_VSIZE, WU_PER_VBYTE, fixed_time};
use shared::api::{FeerateDiagramPoint, MempoolFeerateDiagram};
use shared::events::{ClusterRef, MempoolDeltaEvent};
use time::OffsetDateTime;

pub struct ClusterRefFixture {
    id: i64,
    txids: Vec<String>,
    total_vsize: i64,
    total_fee: i64,
    first_seen_at: Option<OffsetDateTime>,
}

impl ClusterRefFixture {
    pub fn new(id: i64) -> Self {
        Self {
            id,
            txids: vec!["a".into(), "b".into()],
            total_vsize: 2 * TX_VSIZE,
            total_fee: 2 * TX_FEE,
            first_seen_at: Some(fixed_time()),
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

    pub fn with_first_seen_at(mut self, first_seen_at: Option<OffsetDateTime>) -> Self {
        self.first_seen_at = first_seen_at;
        self
    }

    pub fn build(self) -> ClusterRef {
        ClusterRef {
            id: self.id,
            txids: self.txids,
            total_vsize: self.total_vsize,
            total_fee: self.total_fee,
            first_seen_at: self.first_seen_at,
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

/// Raw cumulative feerate diagram, as `getmempoolfeeratediagram` returns it.
pub struct FeerateDiagramFixture {
    sampled_at: Option<OffsetDateTime>,
    points: Vec<FeerateDiagramPoint>,
}

impl Default for FeerateDiagramFixture {
    fn default() -> Self {
        Self {
            sampled_at: Some(fixed_time()),
            // Cumulative and starts at the origin, like the real RPC.
            points: vec![
                FeerateDiagramPoint {
                    weight: 0,
                    fee_sats: 0,
                },
                FeerateDiagramPoint {
                    weight: WU_PER_VBYTE * TX_VSIZE as u64,
                    fee_sats: TX_FEE,
                },
                FeerateDiagramPoint {
                    weight: 2 * WU_PER_VBYTE * TX_VSIZE as u64,
                    fee_sats: 2 * TX_FEE,
                },
            ],
        }
    }
}

impl FeerateDiagramFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sampled_at(mut self, sampled_at: Option<OffsetDateTime>) -> Self {
        self.sampled_at = sampled_at;
        self
    }

    pub fn with_points(mut self, points: Vec<FeerateDiagramPoint>) -> Self {
        self.points = points;
        self
    }

    pub fn build(self) -> MempoolFeerateDiagram {
        MempoolFeerateDiagram {
            sampled_at: self.sampled_at,
            points: self.points,
        }
    }
}
