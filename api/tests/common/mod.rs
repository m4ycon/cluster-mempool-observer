use shared::models::GetRawTransactionModel;
use time::OffsetDateTime;

pub fn dummy_tx(txid: &str) -> GetRawTransactionModel {
    GetRawTransactionModel {
        txid: txid.to_string(),
        version: 2,
        lock_time: u32::MAX,
        vsize: 141,
        weight: 561,
        input_count: 1,
        input_txids: vec!["parent-a".into(), "parent-b".into()],
        output_count: 2,
        confirmations: 0,
        time: Some(OffsetDateTime::UNIX_EPOCH),
    }
}
