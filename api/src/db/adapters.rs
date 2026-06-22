use crate::db::models::NewTransaction;
use shared::models::GetRawTransactionModel;

impl From<GetRawTransactionModel> for NewTransaction {
    fn from(m: GetRawTransactionModel) -> Self {
        Self {
            txid: m.txid,
            version: m.version,
            lock_time: m.lock_time.into(),
            vsize: m.vsize.into(),
            weight: m.weight as i64,
            input_count: m.input_count.into(),
            input_txids: m.input_txids,
            output_count: m.output_count.into(),
            confirmations: m.confirmations as i64,
            time: m.time,
        }
    }
}
