use crate::db::models::{NewMempoolDelta, NewTransaction};
use shared::{events::MempoolDeltaEvent, models::GetRawTransactionModel};

impl From<GetRawTransactionModel> for NewTransaction {
    fn from(m: GetRawTransactionModel) -> Self {
        Self {
            txid: m.txid,
            fee: None,
            vsize: m.vsize.into(),
            first_seen_at: m.time,
            confirmed_at: None,
            cluster_id: None,
        }
    }
}

impl From<&GetRawTransactionModel> for NewTransaction {
    fn from(m: &GetRawTransactionModel) -> Self {
        Self {
            txid: m.txid.clone(),
            fee: None,
            vsize: i64::from(m.vsize),
            first_seen_at: m.time,
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
