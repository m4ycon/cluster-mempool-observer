use observer::error::ObserverError;
use observer::retrievers::TransactionRetriever;
use shared::models::GetRawTransactionModel;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct MockTransactionRetriever {
    txs_fetched: Arc<Mutex<Vec<String>>>,
}

impl MockTransactionRetriever {
    pub fn txs_fetched(&self) -> Vec<String> {
        let mut ids = self.txs_fetched.lock().unwrap().clone();
        ids.sort();
        ids
    }
}

impl TransactionRetriever for MockTransactionRetriever {
    async fn get_raw_transaction(
        &self,
        txid: &str,
    ) -> Result<GetRawTransactionModel, ObserverError> {
        self.txs_fetched.lock().unwrap().push(txid.to_string());
        Ok(GetRawTransactionModel {
            txid: txid.to_string(),
            version: 0,
            lock_time: 0,
            vsize: 0,
            weight: 0,
            input_count: 0,
            input_txids: vec![],
            output_count: 0,
            confirmations: 0,
            time: None,
        })
    }
}
