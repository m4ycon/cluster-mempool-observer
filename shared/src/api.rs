//! HTTP request/response DTOs served by the `api` crate, as the counterpart
//! to `ws.rs`'s websocket surface.
use crate::TS_EXPORT_DIR;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use ts_rs::TS;

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

/// The lifecycle event kinds recorded in the `system_events` table.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub enum SystemEventKind {
    ServerStarted,
    ServerStopped,
    BootstrapStarted,
    BootstrapCompleted,
    NodeConnected,
    NodeDisconnected,
    NodeVersionChanged,
}

/// A row from the `system_events` append-only lifecycle log.
#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct SystemEvent {
    #[ts(type = "number")]
    pub id: i64,
    pub kind: SystemEventKind,
    #[ts(type = "Record<string, unknown>")]
    pub details: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    #[ts(type = "string")]
    pub created_at: OffsetDateTime,
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
