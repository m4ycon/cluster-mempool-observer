use crate::db::models::{NewMempoolDelta, NewTransaction};
use shared::{events::MempoolDeltaEvent, models::GetRawTransactionModel};
use time::OffsetDateTime;

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
