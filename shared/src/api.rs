//! HTTP request/response DTOs served by the `api` crate, as the counterpart
//! to `ws.rs`'s websocket surface.
use crate::TS_EXPORT_DIR;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct TransactionRef {
    pub txid: String,
    #[ts(type = "number | null")]
    pub fee: Option<i64>,
    #[ts(type = "number")]
    pub vsize: i64,
    #[serde(with = "time::serde::rfc3339")]
    #[ts(type = "string")]
    pub first_seen_at: OffsetDateTime,
    #[ts(type = "number | null")]
    pub cluster_id: Option<i64>,
    pub hollow: bool,
    #[ts(type = "Array<string> | null")]
    pub input_txids: Option<Vec<String>>,
}

/// A partial-success lookup: `found` comes back in the order requested, `missing` lists
/// txids this node has no row for -- a miss is not an error, since the client already
/// holds the txid.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct TransactionLookup {
    pub found: Vec<TransactionRef>,
    pub missing: Vec<String>,
}

/// A single `mempool_gauge_samples` column a client can request as its own series.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub enum GaugeMetric {
    ClusterCount,
    ClusteredTxCount,
    MempoolTxCount,
    TotalVsize,
    TotalFee,
}

/// One sampled point of a single-metric series.
#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct GaugePoint {
    #[serde(with = "time::serde::rfc3339")]
    #[ts(type = "string")]
    pub sampled_at: OffsetDateTime,
    #[ts(type = "number")]
    pub value: i64,
}

/// A single-metric projection of `mempool_gauge_samples`, at the resolution the server picked.
#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct GaugeSeries {
    pub metric: GaugeMetric,
    #[ts(type = "number")]
    pub resolution_secs: i64,
    pub points: Vec<GaugePoint>,
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
    fn transaction_ref_distinguishes_none_from_empty_input_txids() {
        let unknown = TransactionRef {
            txid: "a".repeat(64),
            fee: None,
            vsize: 0,
            first_seen_at: OffsetDateTime::UNIX_EPOCH,
            cluster_id: None,
            hollow: true,
            input_txids: None,
        };
        let coinbase = TransactionRef {
            input_txids: Some(vec![]),
            ..unknown.clone()
        };

        let unknown_value = serde_json::to_value(&unknown).unwrap();
        let coinbase_value = serde_json::to_value(&coinbase).unwrap();
        let unknown_obj = unknown_value.as_object().unwrap();

        assert!(unknown_obj.contains_key("input_txids"));
        assert_eq!(unknown_value["input_txids"], serde_json::Value::Null);
        assert!(unknown_obj.contains_key("fee"));
        assert_eq!(unknown_value["fee"], serde_json::Value::Null);
        assert!(unknown_obj.contains_key("cluster_id"));
        assert_eq!(unknown_value["cluster_id"], serde_json::Value::Null);
        assert_eq!(coinbase_value["input_txids"], serde_json::json!([]));
    }

    #[test]
    fn default_is_unsampled_and_empty() {
        let diagram = MempoolFeerateDiagram::default();

        assert_eq!(diagram.sampled_at, None);
        assert!(diagram.points.is_empty());
    }
}
