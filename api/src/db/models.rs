use crate::db::schema::{mempool_deltas, transactions};
use diesel::prelude::*;
use time::OffsetDateTime;

// region: transactions
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = transactions)]
pub struct NewTransaction {
    pub txid: String,
    pub fee: Option<i64>,
    pub vsize: i64,
    pub first_seen_at: Option<OffsetDateTime>,
    pub confirmed_at: Option<OffsetDateTime>,
}

impl NewTransaction {
    pub fn hollow(txid: &str) -> Self {
        Self {
            txid: txid.to_string(),
            fee: None,
            vsize: 0,
            first_seen_at: None,
            confirmed_at: None,
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
