use crate::db::schema::{mempool_deltas, transactions};
use diesel::prelude::*;
use shared::events::MempoolDeltaEvent;
use shared::models::GetRawTransactionModel;
use time::OffsetDateTime;

// region: transactions
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = transactions)]
pub struct NewTransaction {
    pub txid: String,
    pub version: i32,
    pub lock_time: i64,
    pub vsize: i64,
    pub weight: i64,
    pub input_count: i64,
    pub input_txids: Vec<String>,
    pub output_count: i64,
    pub confirmations: i64,
    pub time: Option<OffsetDateTime>,
}

impl NewTransaction {
    pub fn hollow(txid: &str) -> Self {
        Self {
            txid: txid.to_string(),
            version: 0,
            lock_time: 0,
            vsize: 0,
            weight: 0,
            input_count: 0,
            input_txids: Vec::new(),
            output_count: 0,
            confirmations: 0,
            time: None,
        }
    }
}

impl From<&GetRawTransactionModel> for NewTransaction {
    fn from(m: &GetRawTransactionModel) -> Self {
        Self {
            txid: m.txid.clone(),
            version: m.version,
            lock_time: i64::from(m.lock_time),
            vsize: i64::from(m.vsize),
            weight: m.weight as i64,
            input_count: i64::from(m.input_count),
            input_txids: m.input_txids.clone(),
            output_count: i64::from(m.output_count),
            confirmations: m.confirmations as i64,
            time: m.time,
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

impl From<&MempoolDeltaEvent> for NewMempoolDelta {
    fn from(e: &MempoolDeltaEvent) -> Self {
        Self {
            added: e.added.clone(),
            removed: e.removed.clone(),
        }
    }
}
// endregion: mempool_deltas
