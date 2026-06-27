use crate::db::models::{NewBlock, NewMempoolDelta, NewTransaction};
use shared::{
    events::MempoolDeltaEvent,
    models::{GetBlockModel, GetRawTransactionModel},
};
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

impl From<&GetRawTransactionModel> for NewTransaction {
    fn from(m: &GetRawTransactionModel) -> Self {
        Self {
            txid: m.txid.clone(),
            fee: None,
            vsize: i64::from(m.vsize),
            first_seen_at: m.time.unwrap_or_else(OffsetDateTime::now_utc),
            confirmed_at: None,
            cluster_id: None,
        }
    }
}

impl From<&MempoolDeltaEvent> for NewMempoolDelta {
    fn from(e: &MempoolDeltaEvent) -> Self {
        Self {
            added: e.added.clone(),
            removed: e.removed.clone(),
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
        GetRawTransactionModel {
            txid: "deadbeef".to_string(),
            version: 2,
            lock_time: u32::MAX,
            vsize: 141,
            weight: 561,
            input_count: 1,
            input_txids: vec!["parent".into()],
            output_count: 2,
            confirmations: 0,
            time,
        }
    }

    #[test]
    fn from_ref_uses_time_when_present() {
        let t = OffsetDateTime::UNIX_EPOCH;
        let tx = NewTransaction::from(&raw_tx(Some(t)));
        assert_eq!(tx.first_seen_at, t);
    }

    #[test]
    fn from_ref_falls_back_when_time_missing() {
        let before = OffsetDateTime::now_utc();
        let tx = NewTransaction::from(&raw_tx(None));
        assert!(tx.first_seen_at >= before);
    }

    #[test]
    fn hollow_always_fills_first_seen_at() {
        let before = OffsetDateTime::now_utc();
        let tx = NewTransaction::hollow("deadbeef");
        assert!(tx.first_seen_at >= before);
    }
}
