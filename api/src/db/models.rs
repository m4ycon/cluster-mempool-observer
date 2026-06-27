use crate::db::schema::{blocks, clusters, mempool_deltas, transactions};
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
}

impl NewTransaction {
    pub fn hollow(txid: &str) -> Self {
        Self {
            txid: txid.to_string(),
            fee: None,
            vsize: 0,
            first_seen_at: OffsetDateTime::now_utc(),
            confirmed_at: None,
            cluster_id: None,
        }
    }
}
// endregion: transactions

// region: mempool_deltas
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = mempool_deltas)]
pub struct NewMempoolDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
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
