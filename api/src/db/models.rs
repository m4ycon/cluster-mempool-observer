use crate::db::schema::{
    blocks, cluster_deltas, clusters, mempool_deltas, mempool_gauge_samples, system_events,
    transactions,
};
use diesel::prelude::*;
use time::OffsetDateTime;

// region: blocks
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = blocks)]
pub struct NewBlock {
    pub hash: String,
    pub height: i64,
    pub mined_at: OffsetDateTime,
    pub tx_count: i64,
    pub total_bytes: i64,
    pub total_fee: i64,
    pub difficulty: f64,
}
// endregion: blocks

// region: transactions
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = transactions)]
pub struct NewTransaction {
    pub txid: String,
    pub fee: Option<i64>,
    pub vsize: i64,
    pub first_seen_at: OffsetDateTime,
    pub confirmed_at: Option<OffsetDateTime>,
    pub cluster_id: Option<i64>,
    pub confirmed_at_block: Option<String>,
    pub hollow: bool,
    /// Parent txids spent by this tx. `None` means never learned; `Some(vec![])`
    /// means it spends nothing (coinbase).
    pub input_txids: Option<Vec<String>>,
}

impl NewTransaction {
    /// Placeholder for a txid we know is in the mempool but could not retrieve.
    pub fn hollow(txid: &str) -> Self {
        Self {
            txid: txid.to_string(),
            fee: None,
            vsize: 0,
            first_seen_at: OffsetDateTime::now_utc(),
            confirmed_at: None,
            cluster_id: None,
            confirmed_at_block: None,
            hollow: true,
            input_txids: None,
        }
    }

    /// Whether the node still has something to tell us about this row (missing data).
    pub fn needs_backfill(&self) -> bool {
        self.input_txids.is_none() || self.vsize == 0
    }
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = transactions)]
pub struct Transaction {
    pub txid: String,
    pub fee: Option<i64>,
    pub vsize: i64,
    pub first_seen_at: OffsetDateTime,
    pub confirmed_at: Option<OffsetDateTime>,
    pub cluster_id: Option<i64>,
    pub confirmed_at_block: Option<String>,
    pub hollow: bool,
    /// `diesel print-schema` reverts this to `Array<Nullable<Text>>` and must be hand-repatched.
    pub input_txids: Option<Vec<String>>,
}
// endregion: transactions

// region: mempool_deltas
pub use shared::models::DeltaDirection;

/// Why a txid entered or left the mempool
#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::db::schema::sql_types::DeltaReason"]
#[DbValueStyle = "snake_case"]
pub enum DeltaReason {
    /// New tx entered the mempool
    AddMempool,
    /// Tx that was in our mempool, confirmed in a mined block
    RemoveConfirmed,
    /// Tx that was in our mempool, left without a known confirmation
    RemoveEvicted,
}

impl DeltaReason {
    pub const ALL: [DeltaReason; 3] = [
        DeltaReason::AddMempool,
        DeltaReason::RemoveConfirmed,
        DeltaReason::RemoveEvicted,
    ];

    pub fn direction(self) -> DeltaDirection {
        match self {
            DeltaReason::AddMempool => DeltaDirection::Add,
            DeltaReason::RemoveConfirmed | DeltaReason::RemoveEvicted => DeltaDirection::Remove,
        }
    }

    pub fn with_direction(direction: DeltaDirection) -> impl Iterator<Item = DeltaReason> {
        Self::ALL
            .into_iter()
            .filter(move |reason| reason.direction() == direction)
    }
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = mempool_deltas)]
pub struct NewMempoolDelta {
    pub txid: String,
    pub reason: DeltaReason,
}
// endregion: mempool_deltas

// region: clusters
/// A cluster's lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::db::schema::sql_types::ClusterStatus"]
#[DbValueStyle = "snake_case"]
pub enum ClusterStatus {
    /// In the mempool.
    Active,
    /// Confirmed in a mined block.
    Confirmed,
    /// Every member of the cluster has left the mempool without being confirmed.
    Evicted,
    /// This cluster has been merged into another cluster.
    Merged,
}

impl ClusterStatus {
    pub const ALL: [ClusterStatus; 4] = [
        ClusterStatus::Active,
        ClusterStatus::Confirmed,
        ClusterStatus::Evicted,
        ClusterStatus::Merged,
    ];
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = clusters)]
pub struct NewCluster {
    pub txids: Vec<String>,
    pub total_vsize: i64,
    pub total_fee: i64,
    pub first_seen_at: OffsetDateTime,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = clusters)]
pub struct Cluster {
    pub id: i64,
    pub txids: Vec<String>,
    pub total_vsize: i64,
    pub total_fee: i64,
    pub first_seen_at: OffsetDateTime,
    pub confirmed_at: Option<OffsetDateTime>,
    pub status: ClusterStatus,
}
// endregion: clusters

// region: cluster_deltas
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = cluster_deltas)]
pub struct NewClusterDelta {
    pub cluster_id: i64,
    pub added_txids: Vec<String>,
    pub removed_txids: Vec<String>,
    pub fee_delta: i64,
    pub vsize_delta: i64,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = cluster_deltas)]
pub struct ClusterDelta {
    pub id: i64,
    pub cluster_id: i64,
    pub added_txids: Vec<String>,
    pub removed_txids: Vec<String>,
    pub fee_delta: i64,
    pub vsize_delta: i64,
    pub created_at: OffsetDateTime,
}
// endregion: cluster_deltas

// region: mempool_gauge_samples
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = mempool_gauge_samples)]
pub struct NewMempoolGaugeSampleRow {
    pub sampled_at: OffsetDateTime,
    pub cluster_count: i32,
    pub clustered_tx_count: i32,
    pub mempool_tx_count: i32,
    pub total_vsize: i64,
    pub total_fee: i64,
}

#[derive(Debug, Clone, Queryable, Selectable, QueryableByName)]
#[diesel(table_name = mempool_gauge_samples)]
pub struct MempoolGaugeSampleRow {
    pub sampled_at: OffsetDateTime,
    pub cluster_count: i32,
    pub clustered_tx_count: i32,
    pub mempool_tx_count: i32,
    pub total_vsize: i64,
    pub total_fee: i64,
}
// endregion: mempool_gauge_samples

// region: system_events
#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::db::schema::sql_types::SystemEventKind"]
#[DbValueStyle = "snake_case"]
pub enum SystemEventKind {
    ServerStarted,
    ServerStopped,
    BootstrapStarted,
    BootstrapCompleted,
    NodeConnected,
    NodeDisconnected,
    NodeVersionChanged,
}

impl SystemEventKind {
    pub const ALL: [SystemEventKind; 7] = [
        SystemEventKind::ServerStarted,
        SystemEventKind::ServerStopped,
        SystemEventKind::BootstrapStarted,
        SystemEventKind::BootstrapCompleted,
        SystemEventKind::NodeConnected,
        SystemEventKind::NodeDisconnected,
        SystemEventKind::NodeVersionChanged,
    ];
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = system_events)]
pub struct NewSystemEvent {
    pub kind: SystemEventKind,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Queryable, Selectable, QueryableByName)]
#[diesel(table_name = system_events)]
pub struct SystemEventRow {
    pub id: i64,
    pub kind: SystemEventKind,
    pub details: serde_json::Value,
    pub created_at: OffsetDateTime,
}
// endregion: system_events

#[cfg(test)]
mod tests {
    use super::{ClusterStatus, DeltaDirection, DeltaReason, SystemEventKind};

    #[test]
    fn all_covers_every_variant() {
        assert_eq!(DeltaReason::ALL.len(), 3);
    }

    #[test]
    fn system_event_kind_all_covers_every_variant() {
        assert_eq!(SystemEventKind::ALL.len(), 7);
    }

    #[test]
    fn cluster_status_all_covers_every_variant() {
        assert_eq!(ClusterStatus::ALL.len(), 4);
    }

    #[test]
    fn with_direction_selects_matching_reasons() {
        let adds: Vec<DeltaReason> = DeltaReason::with_direction(DeltaDirection::Add).collect();
        assert_eq!(adds, vec![DeltaReason::AddMempool]);

        let removes: Vec<DeltaReason> =
            DeltaReason::with_direction(DeltaDirection::Remove).collect();
        assert_eq!(
            removes,
            vec![DeltaReason::RemoveConfirmed, DeltaReason::RemoveEvicted]
        );
    }
}
