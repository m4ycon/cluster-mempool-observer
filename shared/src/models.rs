use crate::events::FeerateDiagramPoint;
use corepc_client::bitcoin::amount::ParseAmountError;
use corepc_client::bitcoin::{Amount, Weight};
use corepc_client::types::model::GetRawMempoolVerbose;
use corepc_client::types::v31::{
    GetBlockVerboseTwo, GetRawTransactionVerbose, RawTransactionInput,
};
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

impl GetMempoolClusterModel {
    pub fn total_vsize(&self) -> i64 {
        Weight::from_wu(self.cluster_weight).to_vbytes_ceil() as i64
    }
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

#[derive(Deserialize)]
pub struct GetMempoolClusterRaw {
    pub clusterweight: u64,
    pub txcount: u64,
    pub chunks: Vec<ClusterChunkRaw>,
}

#[derive(Deserialize)]
pub struct ClusterChunkRaw {
    /// Fee of the chunk in BTC
    pub chunkfee: f64,
    pub txs: Vec<String>,
}

impl From<&GetMempoolClusterRaw> for GetMempoolClusterModel {
    fn from(response: &GetMempoolClusterRaw) -> Self {
        let txids = response
            .chunks
            .iter()
            .flat_map(|chunk| chunk.txs.iter().cloned())
            .collect();
        let total_fee_sats = response
            .chunks
            .iter()
            .map(|chunk| {
                Amount::from_btc(chunk.chunkfee)
                    .map(|a| a.to_sat())
                    .unwrap_or_default()
            })
            .sum();

        Self {
            cluster_weight: response.clusterweight,
            tx_count: response.txcount as u32,
            txids,
            total_fee_sats,
        }
    }
}

#[derive(Deserialize)]
pub struct FeerateDiagramPointRaw {
    pub weight: u64,
    /// Cumulative fee in BTC
    pub fee: f64,
}

/// `getmempoolfeeratediagram` returns a bare JSON array, not an object.
#[derive(Deserialize)]
pub struct GetMempoolFeerateDiagramRaw(pub Vec<FeerateDiagramPointRaw>);

/// Fallible because the points are cumulative: defaulting an unparseable fee to
/// zero would break monotonicity and read downstream as a negative feerate. A bad
/// point fails the whole poll instead, leaving the last good diagram in place.
impl TryFrom<&GetMempoolFeerateDiagramRaw> for Vec<FeerateDiagramPoint> {
    type Error = ParseAmountError;

    fn try_from(response: &GetMempoolFeerateDiagramRaw) -> Result<Self, Self::Error> {
        response
            .0
            .iter()
            .map(|point| {
                Ok(FeerateDiagramPoint {
                    weight: point.weight,
                    fee_sats: Amount::from_btc(point.fee)?.to_sat() as i64,
                })
            })
            .collect()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GetBlockModel {
    pub hash: String,
    pub height: i64,
    pub mined_at: OffsetDateTime,
    /// Total block size in bytes
    pub size: i64,
    pub difficulty: f64,
    pub txs: Vec<BlockTxSummary>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BlockTxSummary {
    pub txid: String,
    pub vsize: i64,
    pub fee_sats: i64,
    pub input_txids: Vec<String>,
}

impl GetBlockModel {
    pub fn tx_count(&self) -> i64 {
        self.txs.len() as i64
    }

    pub fn total_fee_sats(&self) -> i64 {
        self.txs.iter().map(|tx| tx.fee_sats).sum()
    }
}

impl std::fmt::Debug for GetBlockModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetBlockModel {{ hash: {}, height: {}, tx_count: {} }}",
            self.hash,
            self.height,
            self.txs.len()
        )
    }
}

impl From<&GetBlockVerboseTwo> for GetBlockModel {
    fn from(response: &GetBlockVerboseTwo) -> Self {
        let mined_at = OffsetDateTime::from_unix_timestamp(response.time)
            .unwrap_or_else(|_| OffsetDateTime::now_utc());
        let txs = response
            .tx
            .iter()
            .map(|tx| BlockTxSummary {
                txid: tx.transaction.txid.clone(),
                vsize: tx.transaction.vsize as i64,
                fee_sats: tx
                    .fee
                    .and_then(|btc| Amount::from_btc(btc).ok())
                    .map(|amount| amount.to_sat() as i64)
                    .unwrap_or(0),
                input_txids: parent_txids(&tx.transaction.inputs),
            })
            .collect();

        Self {
            hash: response.hash.clone(),
            height: response.height,
            mined_at,
            size: response.size,
            difficulty: response.difficulty,
            txs,
        }
    }
}

fn parent_txids(inputs: &[RawTransactionInput]) -> Vec<String> {
    inputs
        .iter()
        .filter_map(|input| input.txid.clone())
        .collect()
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
            input_txids: parent_txids(&response.inputs),
            output_count: response.outputs.len() as u32,
            confirmations: response.confirmations.unwrap_or_default(),
            time: response
                .transaction_time
                .and_then(|secs| OffsetDateTime::from_unix_timestamp(secs as i64).ok()),
        }
    }
}

/// Chain state from `getblockchaininfo`, limited to what startup readiness and
/// the health endpoint report.
#[derive(Clone, Serialize, Deserialize)]
pub struct GetBlockchainInfoModel {
    pub blocks: i64,
    pub headers: i64,
    /// Fraction between 0 and 1.
    pub verification_progress: f64,
    /// Whether the node is still in initial block download.
    pub initial_block_download: bool,
}

/// Node version from `getnetworkinfo`, limited to what node-health tracking needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetNetworkInfoModel {
    pub version: usize,
    pub subversion: String,
}

#[cfg(test)]
mod block_tests {
    use super::*;
    use corepc_client::types::v31::{GetBlockVerboseTwoTransaction, GetRawTransactionVerbose};

    /// A vin entry. `None` is what the node sends for a coinbase input.
    fn input(txid: Option<&str>) -> RawTransactionInput {
        RawTransactionInput {
            coinbase: txid.is_none().then(|| "00".to_string()),
            txid: txid.map(|t| t.to_string()),
            vout: Some(0),
            script_sig: None,
            txin_witness: None,
            sequence: 0xffff_ffff,
        }
    }

    fn block_tx_spending(txid: &str, parents: &[Option<&str>]) -> GetBlockVerboseTwoTransaction {
        let mut tx = block_tx(txid, 140, None);
        tx.transaction.inputs = parents.iter().map(|p| input(*p)).collect();
        tx
    }

    fn raw_tx(txid: &str, vsize: u64) -> GetRawTransactionVerbose {
        GetRawTransactionVerbose {
            in_active_chain: None,
            hex: String::new(),
            txid: txid.to_string(),
            hash: String::new(),
            size: vsize,
            vsize,
            weight: vsize * 4,
            version: 2,
            lock_time: 0,
            inputs: vec![],
            outputs: vec![],
            block_hash: None,
            confirmations: Some(1),
            transaction_time: None,
            block_time: None,
        }
    }

    fn block_tx(txid: &str, vsize: u64, fee: Option<f64>) -> GetBlockVerboseTwoTransaction {
        GetBlockVerboseTwoTransaction {
            transaction: raw_tx(txid, vsize),
            fee,
        }
    }

    fn raw_block(txs: Vec<GetBlockVerboseTwoTransaction>) -> GetBlockVerboseTwo {
        GetBlockVerboseTwo {
            hash: "block_hash".to_string(),
            confirmations: 1,
            size: 1234,
            stripped_size: None,
            weight: 4000,
            height: 850_000,
            version: 1,
            version_hex: String::new(),
            merkle_root: String::new(),
            n_tx: txs.len() as i64,
            tx: txs,
            time: 1_700_000_000,
            median_time: None,
            nonce: 0,
            bits: String::new(),
            target: String::new(),
            difficulty: 42.5,
            chain_work: String::new(),
            previous_block_hash: None,
            next_block_hash: None,
        }
    }

    #[test]
    fn maps_block_header_fields() {
        let model = GetBlockModel::from(&raw_block(vec![block_tx("a", 100, None)]));

        assert_eq!(model.hash, "block_hash");
        assert_eq!(model.height, 850_000);
        assert_eq!(model.size, 1234);
        assert_eq!(model.difficulty, 42.5);
        assert_eq!(
            model.mined_at,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
        );
    }

    #[test]
    fn converts_fee_btc_to_sats_and_zeroes_coinbase() {
        // first tx is the coinbase (no fee -> 0), second pays 0.0001 BTC = 10_000 sats
        let model = GetBlockModel::from(&raw_block(vec![
            block_tx("coinbase", 200, None),
            block_tx("b", 140, Some(0.0001)),
        ]));

        assert_eq!(model.txs.len(), 2);
        assert_eq!(model.txs[0].txid, "coinbase");
        assert_eq!(model.txs[0].vsize, 200);
        assert_eq!(model.txs[0].fee_sats, 0);
        assert_eq!(model.txs[1].fee_sats, 10_000);
    }

    #[test]
    fn block_txs_carry_their_parent_txids_and_a_coinbase_carries_none() {
        let model = GetBlockModel::from(&raw_block(vec![
            block_tx_spending("coinbase", &[None]),
            block_tx_spending("b", &[Some("p1"), Some("p2")]),
        ]));

        assert!(model.txs[0].input_txids.is_empty());
        assert_eq!(model.txs[1].input_txids, vec!["p1", "p2"]);
    }

    #[test]
    fn aggregates_tx_count_and_total_fee() {
        let model = GetBlockModel::from(&raw_block(vec![
            block_tx("coinbase", 200, None),
            block_tx("b", 140, Some(0.0001)),
            block_tx("c", 150, Some(0.0002)),
        ]));

        assert_eq!(model.tx_count(), 3);
        assert_eq!(model.total_fee_sats(), 30_000);
    }
}

#[cfg(test)]
mod feerate_diagram_tests {
    use super::*;

    fn try_convert(points: Vec<(u64, f64)>) -> Result<Vec<FeerateDiagramPoint>, ParseAmountError> {
        let raw = GetMempoolFeerateDiagramRaw(
            points
                .into_iter()
                .map(|(weight, fee)| FeerateDiagramPointRaw { weight, fee })
                .collect(),
        );
        (&raw).try_into()
    }

    fn convert(points: Vec<(u64, f64)>) -> Vec<FeerateDiagramPoint> {
        try_convert(points).expect("points convert")
    }

    #[test]
    fn converts_btc_fee_to_integer_sats() {
        let points = convert(vec![(441, 2.712e-05), (878, 0.03708523)]);

        assert_eq!(points[0].weight, 441);
        assert_eq!(points[0].fee_sats, 2712);
        assert_eq!(points[1].weight, 878);
        assert_eq!(points[1].fee_sats, 370_8523);
    }

    #[test]
    fn origin_point_survives_conversion() {
        let points = convert(vec![(0, 0.0)]);

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].weight, 0);
        assert_eq!(points[0].fee_sats, 0);
    }

    #[test]
    fn empty_raw_array_converts_to_empty_points() {
        assert!(convert(vec![]).is_empty());
    }

    #[test]
    fn unrepresentable_fee_fails_instead_of_zeroing_the_point() {
        assert!(try_convert(vec![(441, f64::NAN)]).is_err());
        assert!(try_convert(vec![(441, -1.0)]).is_err());
    }
}
