use corepc_client::types::model::{GetMempoolCluster, GetRawMempoolVerbose};
use corepc_client::types::v31::GetRawTransactionVerbose;
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

impl From<&GetRawMempoolVerbose> for GetRawMempoolVerboseModel {
    fn from(response: &GetRawMempoolVerbose) -> Self {
        let entries = response
            .0
            .iter()
            .map(|(txid, entry)| MempoolEntrySummary {
                txid: txid.to_string(),
                fee_in_sats: entry.fees.base.to_sat(),
                vsize: entry.vsize.unwrap_or_default(),
                ancestor_count: entry.ancestor_count,
                descendant_count: entry.descendant_count,
                time: entry.time,
                height: entry.height,
            })
            .collect();

        Self { entries }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GetMempoolClusterModel {
    pub cluster_weight: u64,
    pub total_fee_sats: u64,
    pub tx_count: u32,
    /// Member txids, flattened from the cluster's chunks in mining order.
    pub txids: Vec<String>,
}

impl std::fmt::Debug for GetMempoolClusterModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetMempoolClusterModel {{ tx_count: {}, total_fee_sats: {} }}",
            self.tx_count, self.total_fee_sats
        )
    }
}

impl From<&GetMempoolCluster> for GetMempoolClusterModel {
    fn from(response: &GetMempoolCluster) -> Self {
        let txids = response
            .chunks
            .iter()
            .flat_map(|chunk| chunk.txs.iter().map(|txid| txid.to_string()))
            .collect();
        let total_fee_sats = response
            .chunks
            .iter()
            .map(|chunk| chunk.chunk_fee.to_sat())
            .sum();

        Self {
            cluster_weight: response.cluster_weight,
            tx_count: response.tx_count as u32,
            txids,
            total_fee_sats,
        }
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

impl From<&GetRawTransactionVerbose> for GetRawTransactionModel {
    fn from(response: &GetRawTransactionVerbose) -> Self {
        Self {
            txid: response.txid.clone(),
            version: response.version,
            lock_time: response.lock_time,
            vsize: response.vsize as u32,
            weight: response.weight,
            input_count: response.inputs.len() as u32,
            input_txids: response
                .inputs
                .iter()
                .filter_map(|input| input.txid.clone())
                .collect(),
            output_count: response.outputs.len() as u32,
            confirmations: response.confirmations.unwrap_or_default(),
            time: response
                .transaction_time
                .and_then(|secs| OffsetDateTime::from_unix_timestamp(secs as i64).ok()),
        }
    }
}
