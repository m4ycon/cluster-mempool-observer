use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use corepc_client::bitcoin::Txid;
use corepc_client::bitcoin::consensus::encode::serialize_hex;
use corepc_client::types::model::GetRawTransaction;
use shared::models::GetRawTransactionModel;

/// On-demand transaction retrievals.
pub struct TransactionRetriever {
    rpc: RpcClient,
}

impl TransactionRetriever {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }

    /// Fetches a transaction by txid via `getrawtransaction`.
    pub async fn get_raw_transaction(
        &self,
        txid: String,
    ) -> Result<GetRawTransactionModel, ObserverError> {
        let txid = txid
            .parse::<Txid>()
            .map_err(|e| ObserverError::InvalidParams(e.to_string()))?;

        let response = self
            .rpc
            .call(move |client| client.get_raw_transaction(txid))
            .await?
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))?;

        Ok(to_model(&response))
    }
}

fn to_model(response: &GetRawTransaction) -> GetRawTransactionModel {
    GetRawTransactionModel {
        txid: response.0.compute_txid().to_string(),
        hex: serialize_hex(&response.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corepc_client::bitcoin::absolute::LockTime;
    use corepc_client::bitcoin::consensus::encode::deserialize_hex;
    use corepc_client::bitcoin::transaction::Version;
    use corepc_client::bitcoin::{Amount, ScriptBuf};
    use corepc_client::bitcoin::{Transaction, TxOut};

    fn dummy_response() -> GetRawTransaction {
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![],
            output: vec![TxOut {
                value: Amount::from_sat(1_000),
                script_pubkey: ScriptBuf::new(),
            }],
        };
        GetRawTransaction(tx)
    }

    #[test]
    fn getrawtransaction_model_txid_should_match_computed_txid() {
        let response = dummy_response();
        let model = to_model(&response);
        assert_eq!(model.txid, response.0.compute_txid().to_string());
    }

    #[test]
    fn getrawtransaction_model_hex_should_round_trip_to_same_tx() {
        let response = dummy_response();
        let model = to_model(&response);

        let decoded: Transaction = deserialize_hex(&model.hex).expect("decode hex");
        assert_eq!(decoded, response.0);
    }
}
