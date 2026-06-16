use serde::{Deserialize, Serialize};

/// A delta of the get_raw_mempool between two consecutive polls
#[derive(Serialize, Deserialize)]
pub struct GetRawMempoolEvent {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl std::fmt::Debug for GetRawMempoolEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetRawMempoolEvent {{ added: {}, removed: {} }}",
            self.added.len(),
            self.removed.len()
        )
    }
}

/// The full mempool fetched via `getrawmempool` with verbose set to true.
#[derive(Serialize, Deserialize)]
pub struct GetRawMempoolVerboseEvent {
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

impl std::fmt::Debug for GetRawMempoolVerboseEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetRawMempoolVerboseEvent {{ entries: {} }}",
            self.entries.len()
        )
    }
}

/// A single transaction fetched via `getrawtransaction`.
#[derive(Serialize, Deserialize)]
pub struct GetRawTransactionEvent {
    pub txid: String,
    pub hex: String,
}

impl std::fmt::Debug for GetRawTransactionEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GetRawTransactionEvent {{ txid: {} }}", self.txid)
    }
}
