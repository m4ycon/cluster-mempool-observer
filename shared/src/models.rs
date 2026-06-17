use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// The full mempool fetched via `getrawmempool` with verbose set to true.
#[derive(Serialize, Deserialize)]
pub struct GetRawMempoolVerboseModel {
    pub entries: Vec<MempoolEntrySummary>,
}

/// A summary of a single mempool entry from `getrawmempool` verbose.
#[derive(Serialize, Deserialize)]
pub struct MempoolEntrySummary {
    pub txid: String,
    pub fee_in_sats: u64,
    pub vsize: u32,
    pub ancestor_count: u32,
    pub descendant_count: u32,
    pub time: u32,
    pub height: u32,
}

impl std::fmt::Debug for GetRawMempoolVerboseModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetRawMempoolVerboseModel {{ entries: {} }}",
            self.entries.len()
        )
    }
}

/// A summary of a single transaction fetched via `getrawtransaction` verbose.
#[derive(Serialize, Deserialize)]
pub struct GetRawTransactionModel {
    pub txid: String,
    pub version: i32,
    pub lock_time: u32,
    pub vsize: u32,
    pub weight: u64,
    pub input_count: u32,
    pub input_txids: Vec<String>,
    pub output_count: u32,
    pub confirmations: u64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub time: Option<OffsetDateTime>,
}

impl std::fmt::Debug for GetRawTransactionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GetRawTransactionModel {{ txid: {} }}", self.txid)
    }
}
