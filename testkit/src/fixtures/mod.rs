//! Builders for the fake domain objects tests assert against.
//!
//! Every builder starts from a sane default and exposes `.with_*()` for the
//! fields a test actually cares about, so a call site only spells out what makes
//! its case distinct. The shared magic numbers live here as consts: assert
//! against `2 * TX_FEE` rather than `1000`, so the intent survives a fixture
//! change.

use time::OffsetDateTime;

/// vsize of a fixture transaction, in vbytes.
pub const TX_VSIZE: i64 = 100;

/// Fee of a fixture transaction, in sats.
pub const TX_FEE: i64 = 500;

/// The fixed point in time fixtures are stamped with.
pub const FIXED_TS: i64 = 1_700_000_000;

/// Weight units per vbyte. A cluster's weight is derived from its members'
/// vsize through this, matching `GetMempoolClusterModel::total_vsize`.
pub const WU_PER_VBYTE: u64 = 4;

/// The canonical fixed timestamp, for tests that need a stable instant.
pub fn fixed_time() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(FIXED_TS).expect("FIXED_TS is a valid unix timestamp")
}

/// A 64-char lowercase-hex txid derived from `seed`, for tests that must pass
/// the API's txid validation.
pub fn hex_txid(seed: &str) -> String {
    format!("{seed:0>64}")
}

pub mod db;
pub mod events;
pub mod models;

pub use db::{
    MempoolDeltaFixture, NewBlockFixture, NewMempoolCounterSampleRowFixture,
    NewMempoolGaugeSampleRowFixture, NewSystemEventFixture, TxFixture, seed_sized_txs, seed_txs,
};
pub use events::{ClusterRefFixture, FeerateDiagramFixture, MempoolDeltaEventFixture};
pub use models::{
    BlockFixture, BlockchainInfoFixture, ClusterFixture, MempoolEntryFixture, RawTxFixture,
};
