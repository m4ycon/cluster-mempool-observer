use super::{FIXED_TS, TX_FEE, TX_VSIZE, WU_PER_VBYTE, fixed_time};
use shared::models::{
    BlockTxSummary, GetBlockModel, GetBlockchainInfoModel, GetMempoolClusterModel,
    GetRawTransactionModel, MempoolEntrySummary,
};
use time::OffsetDateTime;

/// A mempool cluster as the node reports it.
pub struct ClusterFixture {
    txids: Vec<String>,
    total_fee_sats: u64,
    vsize_per_tx: i64,
}

impl ClusterFixture {
    pub fn new(txids: &[&str]) -> Self {
        Self {
            txids: txids.iter().map(|s| s.to_string()).collect(),
            total_fee_sats: txids.len() as u64 * TX_FEE as u64,
            vsize_per_tx: TX_VSIZE,
        }
    }

    pub fn with_total_fee_sats(mut self, total_fee_sats: u64) -> Self {
        self.total_fee_sats = total_fee_sats;
        self
    }

    pub fn with_vsize_per_tx(mut self, vsize_per_tx: i64) -> Self {
        self.vsize_per_tx = vsize_per_tx;
        self
    }

    pub fn build(self) -> GetMempoolClusterModel {
        GetMempoolClusterModel {
            cluster_weight: WU_PER_VBYTE * self.vsize_per_tx as u64 * self.txids.len() as u64,
            tx_count: self.txids.len() as u32,
            total_fee_sats: self.total_fee_sats,
            txids: self.txids,
        }
    }
}

/// A mined block as the node reports it.
pub struct BlockFixture {
    hash: String,
    height: i64,
    mined_at: OffsetDateTime,
    size: i64,
    difficulty: f64,
    txs: Vec<BlockTxSummary>,
}

impl BlockFixture {
    pub fn new(hash: &str, height: i64) -> Self {
        Self {
            hash: hash.to_string(),
            height,
            mined_at: fixed_time(),
            size: 1_000,
            difficulty: 2.0,
            txs: Vec::new(),
        }
    }

    pub fn with_mined_at(mut self, mined_at: OffsetDateTime) -> Self {
        self.mined_at = mined_at;
        self
    }

    /// Member txs as `(txid, fee_sats)` pairs, each stamped with [`TX_VSIZE`].
    pub fn with_txs(mut self, txs: &[(&str, i64)]) -> Self {
        self.txs = txs
            .iter()
            .map(|(txid, fee_sats)| BlockTxSummary {
                txid: txid.to_string(),
                vsize: TX_VSIZE,
                fee_sats: *fee_sats,
            })
            .collect();
        self
    }

    pub fn with_size(mut self, size: i64) -> Self {
        self.size = size;
        self
    }

    pub fn with_difficulty(mut self, difficulty: f64) -> Self {
        self.difficulty = difficulty;
        self
    }

    pub fn build(self) -> GetBlockModel {
        GetBlockModel {
            hash: self.hash,
            height: self.height,
            mined_at: self.mined_at,
            size: self.size,
            difficulty: self.difficulty,
            txs: self.txs,
        }
    }
}

/// A raw transaction as the node reports it.
pub struct RawTxFixture {
    txid: String,
    version: i32,
    lock_time: u32,
    vsize: u32,
    weight: u64,
    input_txids: Vec<String>,
    output_count: u32,
    confirmations: u64,
    time: Option<OffsetDateTime>,
}

impl RawTxFixture {
    pub fn new(txid: &str) -> Self {
        Self {
            txid: txid.to_string(),
            version: 2,
            lock_time: u32::MAX,
            vsize: 141,
            weight: 561,
            input_txids: vec!["parent-a".into(), "parent-b".into()],
            output_count: 2,
            confirmations: 0,
            time: Some(OffsetDateTime::UNIX_EPOCH),
        }
    }

    pub fn with_vsize(mut self, vsize: u32) -> Self {
        self.vsize = vsize;
        self
    }

    pub fn with_weight(mut self, weight: u64) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_input_txids(mut self, input_txids: &[&str]) -> Self {
        self.input_txids = input_txids.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_confirmations(mut self, confirmations: u64) -> Self {
        self.confirmations = confirmations;
        self
    }

    pub fn with_time(mut self, time: Option<OffsetDateTime>) -> Self {
        self.time = time;
        self
    }

    pub fn build(self) -> GetRawTransactionModel {
        GetRawTransactionModel {
            txid: self.txid,
            version: self.version,
            lock_time: self.lock_time,
            vsize: self.vsize,
            weight: self.weight,
            input_count: 1,
            input_txids: self.input_txids,
            output_count: self.output_count,
            confirmations: self.confirmations,
            time: self.time,
        }
    }
}

/// One entry of `getrawmempool verbose`.
pub struct MempoolEntryFixture {
    txid: String,
    fee_in_sats: u64,
    vsize: u32,
    ancestor_count: u32,
    descendant_count: u32,
    time: u32,
    height: u32,
}

impl MempoolEntryFixture {
    pub fn new(txid: &str) -> Self {
        Self {
            txid: txid.to_string(),
            fee_in_sats: TX_FEE as u64,
            vsize: TX_VSIZE as u32,
            ancestor_count: 1,
            descendant_count: 1,
            time: FIXED_TS as u32,
            height: 800_000,
        }
    }

    pub fn with_fee_in_sats(mut self, fee_in_sats: u64) -> Self {
        self.fee_in_sats = fee_in_sats;
        self
    }

    pub fn with_vsize(mut self, vsize: u32) -> Self {
        self.vsize = vsize;
        self
    }

    pub fn with_time(mut self, time: u32) -> Self {
        self.time = time;
        self
    }

    pub fn build(self) -> MempoolEntrySummary {
        MempoolEntrySummary {
            txid: self.txid,
            fee_in_sats: self.fee_in_sats,
            vsize: self.vsize,
            ancestor_count: self.ancestor_count,
            descendant_count: self.descendant_count,
            time: self.time,
            height: self.height,
        }
    }
}

/// Chain state as the node reports it via `getblockchaininfo`.
pub struct BlockchainInfoFixture {
    blocks: i64,
    headers: i64,
    verification_progress: f64,
    initial_block_download: bool,
}

impl BlockchainInfoFixture {
    pub fn new(blocks: i64) -> Self {
        Self {
            blocks,
            headers: blocks,
            verification_progress: 1.0,
            initial_block_download: false,
        }
    }

    pub fn with_initial_block_download(mut self, initial_block_download: bool) -> Self {
        self.initial_block_download = initial_block_download;
        self
    }

    pub fn build(self) -> GetBlockchainInfoModel {
        GetBlockchainInfoModel {
            blocks: self.blocks,
            headers: self.headers,
            verification_progress: self.verification_progress,
            initial_block_download: self.initial_block_download,
        }
    }
}
