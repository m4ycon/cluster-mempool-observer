use crate::db::models::{
    NewBlock, NewTransaction, SystemEventKind as DbSystemEventKind, SystemEventRow,
};
use shared::events::{SystemEvent, SystemEventKind};
use shared::models::{GetBlockModel, GetRawTransactionModel, MempoolEntrySummary};
use time::OffsetDateTime;

impl From<&GetBlockModel> for NewBlock {
    fn from(m: &GetBlockModel) -> Self {
        Self {
            hash: m.hash.clone(),
            height: m.height,
            mined_at: m.mined_at,
            tx_count: m.tx_count(),
            total_bytes: m.size,
            total_fee: m.total_fee_sats(),
            difficulty: m.difficulty,
        }
    }
}

impl From<&MempoolEntrySummary> for NewTransaction {
    fn from(e: &MempoolEntrySummary) -> Self {
        Self {
            txid: e.txid.clone(),
            fee: Some(e.fee_in_sats as i64),
            vsize: i64::from(e.vsize),
            first_seen_at: OffsetDateTime::now_utc(),
            confirmed_at: None,
            cluster_id: None,
            confirmed_at_block: None,
            hollow: false,
        }
    }
}

impl From<&GetRawTransactionModel> for NewTransaction {
    fn from(m: &GetRawTransactionModel) -> Self {
        Self {
            txid: m.txid.clone(),
            fee: None,
            vsize: i64::from(m.vsize),
            first_seen_at: OffsetDateTime::now_utc(),
            confirmed_at: None,
            cluster_id: None,
            confirmed_at_block: None,
            hollow: false,
        }
    }
}

impl From<SystemEventRow> for SystemEvent {
    fn from(row: SystemEventRow) -> Self {
        Self {
            id: row.id,
            kind: row.kind.into(),
            details: row.details,
            created_at: row.created_at,
        }
    }
}

impl From<DbSystemEventKind> for SystemEventKind {
    fn from(kind: DbSystemEventKind) -> Self {
        match kind {
            DbSystemEventKind::ServerStarted => Self::ServerStarted,
            DbSystemEventKind::ServerStopped => Self::ServerStopped,
            DbSystemEventKind::BootstrapStarted => Self::BootstrapStarted,
            DbSystemEventKind::BootstrapCompleted => Self::BootstrapCompleted,
            DbSystemEventKind::NodeConnected => Self::NodeConnected,
            DbSystemEventKind::NodeDisconnected => Self::NodeDisconnected,
            DbSystemEventKind::NodeVersionChanged => Self::NodeVersionChanged,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::NewTransaction;
    use shared::models::BlockTxSummary;

    #[test]
    fn new_block_aggregates_tx_count_and_total_fee() {
        let mined_at = OffsetDateTime::UNIX_EPOCH;
        let model = GetBlockModel {
            hash: "abc".to_string(),
            height: 42,
            mined_at,
            size: 999,
            difficulty: 3.5,
            txs: vec![
                BlockTxSummary {
                    txid: "coinbase".into(),
                    vsize: 200,
                    fee_sats: 0,
                },
                BlockTxSummary {
                    txid: "b".into(),
                    vsize: 140,
                    fee_sats: 1000,
                },
            ],
        };

        let block = NewBlock::from(&model);
        assert_eq!(block.hash, "abc");
        assert_eq!(block.height, 42);
        assert_eq!(block.mined_at, mined_at);
        assert_eq!(block.tx_count, 2);
        assert_eq!(block.total_bytes, 999);
        assert_eq!(block.total_fee, 1000);
        assert_eq!(block.difficulty, 3.5);
    }

    fn raw_tx(time: Option<OffsetDateTime>) -> GetRawTransactionModel {
        testkit::fixtures::RawTxFixture::new("deadbeef")
            .with_input_txids(&["parent"])
            .with_time(time)
            .build()
    }

    #[test]
    fn raw_tx_first_seen_at_is_our_clock_not_the_node_time() {
        let before = OffsetDateTime::now_utc();
        assert!(NewTransaction::from(&raw_tx(None)).first_seen_at >= before);
        assert!(
            NewTransaction::from(&raw_tx(Some(OffsetDateTime::UNIX_EPOCH))).first_seen_at >= before
        );
    }

    #[test]
    fn hollow_always_fills_first_seen_at() {
        let before = OffsetDateTime::now_utc();
        let tx = NewTransaction::hollow("deadbeef");
        assert!(tx.first_seen_at >= before);
    }

    #[test]
    fn hollow_is_flagged_and_retrieved_is_not() {
        assert!(NewTransaction::hollow("deadbeef").hollow);
        assert!(!NewTransaction::from(&raw_tx(None)).hollow);
    }

    #[test]
    fn mempool_entry_carries_fee_and_vsize() {
        let entry = testkit::fixtures::MempoolEntryFixture::new("deadbeef")
            .with_fee_in_sats(1234)
            .with_vsize(250)
            .build();

        let tx = NewTransaction::from(&entry);
        assert_eq!(tx.txid, "deadbeef");
        assert_eq!(tx.fee, Some(1234));
        assert_eq!(tx.vsize, 250);
        assert!(!tx.hollow);
    }

    /// The entry's `time` is the node's acceptance time. `first_seen_at` is ours,
    /// so it must ignore it -- otherwise a bootstrap would backdate every row.
    #[test]
    fn mempool_entry_first_seen_at_is_our_clock_not_the_node_time() {
        let before = OffsetDateTime::now_utc();
        let entry = testkit::fixtures::MempoolEntryFixture::new("deadbeef")
            .with_time(1_600_000_000)
            .build();

        assert!(NewTransaction::from(&entry).first_seen_at >= before);
    }
}
