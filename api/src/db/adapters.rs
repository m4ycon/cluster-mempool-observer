use crate::db::models::{
    NewBlock, NewTransaction, SystemEventKind as DbSystemEventKind, SystemEventRow, Transaction,
};
use shared::api::{SystemEvent, SystemEventKind, TransactionRef};
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
            created_at: OffsetDateTime::now_utc(),
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
            input_txids: None,
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
            input_txids: Some(m.input_txids.clone()),
        }
    }
}

impl From<Transaction> for TransactionRef {
    fn from(row: Transaction) -> Self {
        Self {
            txid: row.txid,
            fee: row.fee,
            vsize: row.vsize,
            first_seen_at: row.first_seen_at,
            cluster_id: row.cluster_id,
            hollow: row.hollow,
            input_txids: row.input_txids,
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

    #[test]
    fn new_block_aggregates_tx_count_and_total_fee() {
        let mined_at = OffsetDateTime::UNIX_EPOCH;
        let model = testkit::fixtures::BlockFixture::new("abc", 42)
            .with_mined_at(mined_at)
            .with_size(999)
            .with_difficulty(3.5)
            .with_txs(&[("coinbase", 0), ("b", 1000)])
            .build();

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
    fn parent_txids_come_from_the_vin_and_hollow_rows_have_none() {
        let raw = testkit::fixtures::RawTxFixture::new("deadbeef")
            .with_input_txids(&["p1", "p2"])
            .build();
        assert_eq!(
            NewTransaction::from(&raw).input_txids,
            Some(vec!["p1".to_string(), "p2".to_string()])
        );

        let entry = testkit::fixtures::MempoolEntryFixture::new("deadbeef").build();
        assert_eq!(NewTransaction::from(&entry).input_txids, None);

        assert_eq!(NewTransaction::hollow("deadbeef").input_txids, None);
    }

    #[test]
    fn hollow_is_flagged_and_retrieved_is_not() {
        assert!(NewTransaction::hollow("deadbeef").hollow);
        assert!(!NewTransaction::from(&raw_tx(None)).hollow);
    }

    #[test]
    fn only_rows_the_node_can_still_enrich_need_backfill() {
        let entry = testkit::fixtures::MempoolEntryFixture::new("deadbeef").build();
        assert!(NewTransaction::from(&entry).needs_backfill());
        assert!(NewTransaction::hollow("deadbeef").needs_backfill());
        assert!(!NewTransaction::from(&raw_tx(None)).needs_backfill());
    }

    #[test]
    fn sorted_by_txid_orders_ascending() {
        let txs = vec![
            NewTransaction::hollow("c"),
            NewTransaction::hollow("a"),
            NewTransaction::hollow("b"),
        ];

        let sorted = NewTransaction::sorted_by_txid(&txs);

        let txids: Vec<&str> = sorted.iter().map(|tx| tx.txid.as_str()).collect();
        assert_eq!(txids, vec!["a", "b", "c"]);
    }

    #[test]
    fn a_row_with_parents_but_no_vsize_still_needs_backfill() {
        let mut tx = NewTransaction::from(&raw_tx(None));
        tx.vsize = 0;
        assert!(tx.needs_backfill());
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

    fn tx_row(input_txids: Option<Vec<String>>) -> Transaction {
        Transaction {
            txid: "deadbeef".to_string(),
            fee: None,
            vsize: 0,
            first_seen_at: OffsetDateTime::UNIX_EPOCH,
            confirmed_at: None,
            cluster_id: None,
            confirmed_at_block: None,
            hollow: true,
            input_txids,
        }
    }

    #[test]
    fn transaction_ref_preserves_the_three_input_txids_states() {
        assert_eq!(TransactionRef::from(tx_row(None)).input_txids, None);
        assert_eq!(
            TransactionRef::from(tx_row(Some(vec![]))).input_txids,
            Some(vec![])
        );
        assert_eq!(
            TransactionRef::from(tx_row(Some(vec!["p".to_string()]))).input_txids,
            Some(vec!["p".to_string()])
        );
    }
}
