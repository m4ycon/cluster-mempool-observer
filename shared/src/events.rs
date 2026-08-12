use crate::TS_EXPORT_DIR;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use ts_rs::TS;

/// A delta of the get_raw_mempool between two consecutive polls
#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct MempoolDeltaEvent {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl std::fmt::Debug for MempoolDeltaEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MempoolDeltaEvent {{ added: {}, removed: {} }}",
            self.added.len(),
            self.removed.len()
        )
    }
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct ClusterRef {
    #[ts(type = "number")]
    pub id: i64,
    pub txids: Vec<String>,
    #[ts(type = "number")]
    pub total_vsize: i64,
    #[ts(type = "number")]
    pub total_fee: i64,
}

#[derive(Serialize, Deserialize, Default, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct ClusterDeltaEvent {
    pub upserted: Vec<ClusterRef>,
    pub removed: Vec<i64>,
}

impl std::fmt::Debug for ClusterDeltaEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ClusterDeltaEvent {{ upserted: {}, removed: {} }}",
            self.upserted.len(),
            self.removed.len()
        )
    }
}

/// A block connected to the chain tip (via ZMQ `hashblock`)
#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct BlockConnectedEvent {
    pub hash: String,
}

impl std::fmt::Debug for BlockConnectedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlockConnectedEvent {{ hash: {} }}", self.hash)
    }
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct MempoolStatsEvent {
    #[ts(type = "number")]
    pub mempool_size: i64,
    #[ts(type = "number")]
    pub cluster_count: i64,
    #[ts(type = "number")]
    pub tx_per_min: i64,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct NewBlockInfoEvent {
    #[ts(type = "number")]
    pub height: i64,
    #[serde(with = "time::serde::rfc3339")]
    #[ts(type = "string")]
    pub mined_at: OffsetDateTime,
}

/// A single `mempool_snapshots` column a client can request as its own series.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub enum SnapshotMetric {
    ClusterCount,
    ClusteredTxCount,
    MempoolTxCount,
    TotalVsize,
    TotalFee,
}

/// One sampled point of a single-metric series.
#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct MempoolMetricPoint {
    #[serde(with = "time::serde::rfc3339")]
    #[ts(type = "string")]
    pub sampled_at: OffsetDateTime,
    #[ts(type = "number")]
    pub value: i64,
}

/// A single-metric projection of `mempool_snapshots`, at the resolution the server picked.
#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct MempoolMetricSeries {
    pub metric: SnapshotMetric,
    #[ts(type = "number")]
    pub resolution_secs: i64,
    pub points: Vec<MempoolMetricPoint>,
}

/// One point of the cumulative feerate diagram from `getmempoolfeeratediagram`.
#[derive(Serialize, Deserialize, Clone, PartialEq, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct FeerateDiagramPoint {
    #[ts(type = "number")]
    pub weight: u64,
    #[ts(type = "number")]
    pub fee_sats: i64,
}

/// Raw cumulative feerate diagram from `getmempoolfeeratediagram`, served as-is.
#[derive(Serialize, Deserialize, Default, Clone, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct MempoolFeerateDiagram {
    #[serde(with = "time::serde::rfc3339::option")]
    #[ts(type = "string | null")]
    pub sampled_at: Option<OffsetDateTime>,
    pub points: Vec<FeerateDiagramPoint>,
}

impl std::fmt::Debug for MempoolFeerateDiagram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MempoolFeerateDiagram {{ points: {}, sampled_at: {:?} }}",
            self.points.len(),
            self.sampled_at
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unsampled_and_empty() {
        let diagram = MempoolFeerateDiagram::default();

        assert_eq!(diagram.sampled_at, None);
        assert!(diagram.points.is_empty());
    }
}
