use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use corepc_client::bitcoin::Txid;
use corepc_client::types::v31::GetRawTransactionVerbose;
use shared::models::GetRawTransactionModel;
use std::future::Future;
use time::OffsetDateTime;

/// On-demand transaction retrievals.
pub trait TransactionRetriever: Clone + Send + Sync {
    /// Fetches a transaction by txid.
    fn get_raw_transaction(
        &self,
        txid: &str,
    ) -> impl Future<Output = Result<GetRawTransactionModel, ObserverError>> + Send;
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
        let txid = txid
            .parse::<Txid>()
            .map_err(|e| ObserverError::InvalidParams(e.to_string()))?;

        let response = self
            .rpc
            .call(move |client| client.get_raw_transaction_verbose(txid))
            .await?;

        Ok(to_model(&response))
    }
}

fn to_model(response: &GetRawTransactionVerbose) -> GetRawTransactionModel {
    GetRawTransactionModel {
        txid: response.txid.clone(),
        version: response.version,
        lock_time: response.lock_time,
        vsize: response.vsize as u32,
        weight: response.weight,
        input_count: response.inputs.len() as u32,
        input_txids: response
            .inputs
            .iter()
            .filter_map(|input| input.txid.clone())
            .collect(),
        output_count: response.outputs.len() as u32,
        confirmations: response.confirmations.unwrap_or_default(),
        time: response
            .transaction_time
            .and_then(|secs| OffsetDateTime::from_unix_timestamp(secs as i64).ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corepc_client::types::v31::RawTransactionInput;

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
        let model = to_model(&response);

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
