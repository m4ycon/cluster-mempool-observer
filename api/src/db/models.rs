use crate::db::schema::{
    blocks, cluster_deltas, clusters, mempool_deltas, mempool_snapshots, transactions,
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
        }
    }
}
// endregion: transactions

// region: mempool_deltas
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaDirection {
    Add,
    Remove,
}

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
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = clusters)]
pub struct NewCluster {
    pub txids: Vec<String>,
    pub total_vsize: i64,
    pub total_fee: i64,
    pub first_seen_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = clusters)]
pub struct Cluster {
    pub id: i64,
    pub txids: Vec<String>,
    pub total_vsize: i64,
    pub total_fee: i64,
    pub first_seen_at: Option<OffsetDateTime>,
    pub confirmed_at: Option<OffsetDateTime>,
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

// region: mempool_snapshots
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = mempool_snapshots)]
pub struct NewMempoolSnapshotRow {
    pub sampled_at: OffsetDateTime,
    pub cluster_count: i32,
    pub clustered_tx_count: i32,
    pub mempool_tx_count: i32,
    pub total_vsize: i64,
    pub total_fee: i64,
}

#[derive(Debug, Clone, Queryable, Selectable, QueryableByName)]
#[diesel(table_name = mempool_snapshots)]
pub struct MempoolSnapshotRow {
    pub sampled_at: OffsetDateTime,
    pub cluster_count: i32,
    pub clustered_tx_count: i32,
    pub mempool_tx_count: i32,
    pub total_vsize: i64,
    pub total_fee: i64,
}
// endregion: mempool_snapshots

#[cfg(test)]
mod tests {
    use super::{DeltaDirection, DeltaReason};

    #[test]
    fn all_covers_every_variant() {
        assert_eq!(DeltaReason::ALL.len(), 3);
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
