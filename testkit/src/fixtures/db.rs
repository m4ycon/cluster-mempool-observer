use super::{TX_VSIZE, fixed_time};
use api::db::TransactionRepository;
use api::db::models::{
    DeltaReason, NewBlock, NewMempoolCounterSampleRow, NewMempoolDelta, NewMempoolGaugeSampleRow,
    NewSystemEvent, NewTransaction, SystemEventKind,
};
use time::OffsetDateTime;

/// A `transactions` row. Starts from `NewTransaction::hollow` -- the production
/// placeholder constructor -- so the fixture never drifts from it.
pub struct TxFixture {
    tx: NewTransaction,
}

impl TxFixture {
    pub fn new(txid: &str) -> Self {
        Self {
            tx: NewTransaction::hollow(txid),
        }
    }

    /// Stamp the standard fixture fee and vsize, so the row carries the totals
    /// cluster arithmetic is asserted against.
    pub fn sized(self) -> Self {
        self.with_fee(Some(super::TX_FEE))
            .with_vsize(TX_VSIZE)
            .with_hollow(false)
    }

    pub fn with_hollow(mut self, hollow: bool) -> Self {
        self.tx.hollow = hollow;
        self
    }

    pub fn with_fee(mut self, fee: Option<i64>) -> Self {
        self.tx.fee = fee;
        self
    }

    pub fn with_vsize(mut self, vsize: i64) -> Self {
        self.tx.vsize = vsize;
        self
    }

    pub fn with_first_seen_at(mut self, first_seen_at: OffsetDateTime) -> Self {
        self.tx.first_seen_at = first_seen_at;
        self
    }

    pub fn with_confirmed_at(mut self, confirmed_at: Option<OffsetDateTime>) -> Self {
        self.tx.confirmed_at = confirmed_at;
        self
    }

    pub fn with_cluster_id(mut self, cluster_id: Option<i64>) -> Self {
        self.tx.cluster_id = cluster_id;
        self
    }

    pub fn with_input_txids(mut self, input_txids: &[&str]) -> Self {
        self.tx.input_txids = Some(input_txids.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn build(self) -> NewTransaction {
        self.tx
    }
}

/// Insert one hollow `transactions` row per txid.
pub async fn seed_txs(repo: &TransactionRepository, txids: &[&str]) {
    for txid in txids {
        repo.insert(&TxFixture::new(txid).build())
            .await
            .expect("seed tx");
    }
}

/// Insert one `transactions` row per txid, each carrying [`super::TX_FEE`] and
/// [`TX_VSIZE`] so cluster totals are derivable from the member count.
pub async fn seed_sized_txs(repo: &TransactionRepository, txids: &[&str]) {
    for txid in txids {
        repo.insert(&TxFixture::new(txid).sized().build())
            .await
            .expect("seed tx");
    }
}

/// A `mempool_deltas` row.
pub struct MempoolDeltaFixture {
    txid: String,
    reason: DeltaReason,
    created_at: OffsetDateTime,
}

impl MempoolDeltaFixture {
    pub fn new(txid: &str, reason: DeltaReason) -> Self {
        Self {
            txid: txid.to_string(),
            reason,
            created_at: OffsetDateTime::now_utc(),
        }
    }

    /// The common case: a tx entering the mempool.
    pub fn added(txid: &str) -> Self {
        Self::new(txid, DeltaReason::AddMempool)
    }

    pub fn with_reason(mut self, reason: DeltaReason) -> Self {
        self.reason = reason;
        self
    }

    pub fn with_created_at(mut self, created_at: OffsetDateTime) -> Self {
        self.created_at = created_at;
        self
    }

    pub fn build(self) -> NewMempoolDelta {
        NewMempoolDelta {
            txid: self.txid,
            reason: self.reason,
            created_at: self.created_at,
        }
    }
}

/// A `blocks` row.
pub struct NewBlockFixture {
    hash: String,
    height: i64,
    mined_at: OffsetDateTime,
    tx_count: i64,
    total_bytes: i64,
    total_fee: i64,
    difficulty: f64,
    created_at: OffsetDateTime,
}

impl NewBlockFixture {
    pub fn new(hash: &str, height: i64) -> Self {
        Self {
            hash: hash.to_string(),
            height,
            mined_at: fixed_time(),
            tx_count: 1,
            total_bytes: 10,
            total_fee: 5,
            difficulty: 1.0,
            created_at: fixed_time(),
        }
    }

    pub fn with_mined_at(mut self, mined_at: OffsetDateTime) -> Self {
        self.mined_at = mined_at;
        self
    }

    pub fn with_tx_count(mut self, tx_count: i64) -> Self {
        self.tx_count = tx_count;
        self
    }

    pub fn with_total_bytes(mut self, total_bytes: i64) -> Self {
        self.total_bytes = total_bytes;
        self
    }

    pub fn with_total_fee(mut self, total_fee: i64) -> Self {
        self.total_fee = total_fee;
        self
    }

    pub fn with_difficulty(mut self, difficulty: f64) -> Self {
        self.difficulty = difficulty;
        self
    }

    pub fn with_created_at(mut self, created_at: OffsetDateTime) -> Self {
        self.created_at = created_at;
        self
    }

    pub fn build(self) -> NewBlock {
        NewBlock {
            hash: self.hash,
            height: self.height,
            mined_at: self.mined_at,
            tx_count: self.tx_count,
            total_bytes: self.total_bytes,
            total_fee: self.total_fee,
            difficulty: self.difficulty,
            created_at: self.created_at,
        }
    }
}

/// A `mempool_gauge_samples` row.
pub struct NewMempoolGaugeSampleRowFixture {
    sampled_at: OffsetDateTime,
    cluster_count: i32,
    clustered_tx_count: i32,
    mempool_tx_count: i32,
    total_vsize: i64,
    total_fee: i64,
}

impl NewMempoolGaugeSampleRowFixture {
    pub fn new(sampled_at: OffsetDateTime) -> Self {
        Self {
            sampled_at,
            cluster_count: 1,
            clustered_tx_count: 2,
            mempool_tx_count: 2,
            total_vsize: 200,
            total_fee: 400,
        }
    }

    pub fn with_cluster_count(mut self, cluster_count: i32) -> Self {
        self.cluster_count = cluster_count;
        self
    }

    pub fn with_clustered_tx_count(mut self, clustered_tx_count: i32) -> Self {
        self.clustered_tx_count = clustered_tx_count;
        self
    }

    pub fn with_mempool_tx_count(mut self, mempool_tx_count: i32) -> Self {
        self.mempool_tx_count = mempool_tx_count;
        self
    }

    pub fn with_total_vsize(mut self, total_vsize: i64) -> Self {
        self.total_vsize = total_vsize;
        self
    }

    pub fn with_total_fee(mut self, total_fee: i64) -> Self {
        self.total_fee = total_fee;
        self
    }

    pub fn build(self) -> NewMempoolGaugeSampleRow {
        NewMempoolGaugeSampleRow {
            sampled_at: self.sampled_at,
            cluster_count: self.cluster_count,
            clustered_tx_count: self.clustered_tx_count,
            mempool_tx_count: self.mempool_tx_count,
            total_vsize: self.total_vsize,
            total_fee: self.total_fee,
        }
    }
}

/// A `mempool_counter_samples` row.
pub struct NewMempoolCounterSampleRowFixture {
    sampled_at: OffsetDateTime,
    period_secs: i64,
    added_txs: Option<i64>,
    confirmed_txs: Option<i64>,
    evicted_txs: Option<i64>,
}

impl NewMempoolCounterSampleRowFixture {
    pub fn new(sampled_at: OffsetDateTime) -> Self {
        Self {
            sampled_at,
            period_secs: 60,
            added_txs: Some(1),
            confirmed_txs: Some(1),
            evicted_txs: Some(1),
        }
    }

    pub fn with_period_secs(mut self, period_secs: i64) -> Self {
        self.period_secs = period_secs;
        self
    }

    pub fn with_added_txs(mut self, added_txs: Option<i64>) -> Self {
        self.added_txs = added_txs;
        self
    }

    pub fn with_confirmed_txs(mut self, confirmed_txs: Option<i64>) -> Self {
        self.confirmed_txs = confirmed_txs;
        self
    }

    pub fn with_evicted_txs(mut self, evicted_txs: Option<i64>) -> Self {
        self.evicted_txs = evicted_txs;
        self
    }

    pub fn build(self) -> NewMempoolCounterSampleRow {
        NewMempoolCounterSampleRow {
            sampled_at: self.sampled_at,
            period_secs: self.period_secs,
            added_txs: self.added_txs,
            confirmed_txs: self.confirmed_txs,
            evicted_txs: self.evicted_txs,
        }
    }
}

/// A `system_events` row.
pub struct NewSystemEventFixture {
    kind: SystemEventKind,
    details: serde_json::Value,
}

impl NewSystemEventFixture {
    pub fn new(kind: SystemEventKind) -> Self {
        Self {
            kind,
            details: serde_json::json!({}),
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }

    pub fn build(self) -> NewSystemEvent {
        NewSystemEvent {
            kind: self.kind,
            details: self.details,
        }
    }
}
