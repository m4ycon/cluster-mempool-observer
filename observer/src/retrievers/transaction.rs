use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use corepc_client::bitcoin::Txid;
use shared::models::{GetRawTransactionModel, MempoolEntrySummary};
use std::future::Future;

pub trait TransactionRetriever: Clone + Send + Sync {
    /// Fetches a transaction by txid.
    fn get_raw_transaction(
        &self,
        txid: &str,
    ) -> impl Future<Output = Result<GetRawTransactionModel, ObserverError>> + Send;

    /// Fetches a mempool transaction's entry via `getmempoolentry`.
    fn get_mempool_entry(
        &self,
        txid: &str,
    ) -> impl Future<Output = Result<MempoolEntrySummary, ObserverError>> + Send;
}

#[derive(Clone)]
pub struct TransactionRpcRetriever {
    rpc: RpcClient,
}

impl TransactionRpcRetriever {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }
}

impl TransactionRetriever for TransactionRpcRetriever {
    async fn get_raw_transaction(
        &self,
        txid: &str,
    ) -> Result<GetRawTransactionModel, ObserverError> {
        let txid = parse_txid(txid)?;
        let response = self
            .rpc
            .call("getrawtransaction", move |client| {
                client.get_raw_transaction_verbose(txid)
            })
            .await
            .map_err(|e| not_found_as_missing(e, "No such mempool transaction"))?;

        Ok(GetRawTransactionModel::from(&response))
    }

    async fn get_mempool_entry(&self, txid: &str) -> Result<MempoolEntrySummary, ObserverError> {
        let txid = parse_txid(txid)?;
        let response = self
            .rpc
            .call("getmempoolentry", move |client| {
                client.get_mempool_entry(txid)
            })
            .await
            .map_err(|e| not_found_as_missing(e, "Transaction not in mempool"))?
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))?;

        Ok(MempoolEntrySummary::new(&txid, &response.0))
    }
}

fn parse_txid(txid: &str) -> Result<Txid, ObserverError> {
    txid.parse::<Txid>()
        .map_err(|e| ObserverError::InvalidParams(e.to_string()))
}

/// Each RPC words "the node does not have this tx" differently, so the caller
/// names the message its method answers with.
fn not_found_as_missing(e: ObserverError, message: &str) -> ObserverError {
    if e.to_string().contains(message) {
        ObserverError::TxNotFoundInMempool(e.to_string())
    } else {
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corepc_client::types::v31::{GetRawTransactionVerbose, RawTransactionInput};
    use time::OffsetDateTime;

    fn dummy_input(txid: Option<String>) -> RawTransactionInput {
        RawTransactionInput {
            coinbase: None,
            txid,
            vout: Some(0),
            script_sig: None,
            txin_witness: None,
            sequence: 0xffff_ffff,
        }
    }

    fn dummy_response() -> GetRawTransactionVerbose {
        GetRawTransactionVerbose {
            in_active_chain: None,
            hex: String::new(),
            txid: "0".repeat(64),
            hash: String::new(),
            size: 200,
            vsize: 140,
            weight: 560,
            version: 2,
            lock_time: 0,
            inputs: vec![dummy_input(Some("a".repeat(64))), dummy_input(None)],
            outputs: vec![],
            block_hash: None,
            confirmations: Some(6),
            transaction_time: Some(1_700_000_000),
            block_time: Some(1_700_000_050),
        }
    }

    #[test]
    fn getrawtransaction_model_maps_verbose_fields() {
        let response = dummy_response();
        let model = GetRawTransactionModel::from(&response);

        assert_eq!(model.txid, response.txid);
        assert_eq!(model.version, 2);
        assert_eq!(model.lock_time, 0);
        assert_eq!(model.vsize, 140);
        assert_eq!(model.weight, 560);
        assert_eq!(model.input_count, 2);
        assert_eq!(model.input_txids, vec!["a".repeat(64)]);
        assert_eq!(model.output_count, 0);
        assert_eq!(model.confirmations, 6);
        assert_eq!(
            model.time,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).ok()
        );
    }
}
