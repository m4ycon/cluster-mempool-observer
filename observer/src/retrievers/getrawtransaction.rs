use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use crate::infra::config::RetrieversConfig;
use crate::retrievers::retrievers_trait::Retriever;
use async_nats::Client;
use corepc_client::bitcoin::Txid;
use corepc_client::bitcoin::consensus::encode::serialize_hex;
use corepc_client::types::model::GetRawTransaction;
use shared::events::GetRawTransactionEvent;
use shared::subjects::Subject;

pub struct GetRawTransactionRetriever {
    nats: Client,
    rpc: RpcClient,
}

impl GetRawTransactionRetriever {
    pub fn new(nats: Client, rpc: RpcClient) -> Self {
        Self { nats, rpc }
    }
}

impl Retriever for GetRawTransactionRetriever {
    type Params = String;
    type Response = GetRawTransaction;
    type Event = GetRawTransactionEvent;

    fn publisher(&self) -> &Client {
        &self.nats
    }

    fn publish_subject(&self) -> Subject {
        Subject::RawTransaction
    }

    fn subscribe_subject(&self) -> Subject {
        Subject::RequestRawTransaction
    }

    fn is_enabled(&self, config: &RetrieversConfig) -> bool {
        config.getrawtransaction
    }

    async fn retrieve(&mut self, params: Self::Params) -> Result<Self::Response, ObserverError> {
        let txid = params
            .parse::<Txid>()
            .map_err(|e| ObserverError::InvalidParams(e.to_string()))?;
        let response = self
            .rpc
            .call(move |client| client.get_raw_transaction(txid))
            .await?;

        response
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))
    }

    fn to_event(response: &Self::Response) -> Self::Event {
        Self::Event {
            txid: response.0.compute_txid().to_string(),
            hex: serialize_hex(&response.0),
        }
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
    fn getrawtransaction_event_txid_should_match_computed_txid() {
        let response = dummy_response();
        let event = GetRawTransactionRetriever::to_event(&response);
        assert_eq!(event.txid, response.0.compute_txid().to_string());
    }

    #[test]
    fn getrawtransaction_event_hex_should_round_trip_to_same_tx() {
        let response = dummy_response();
        let event = GetRawTransactionRetriever::to_event(&response);

        let decoded: Transaction = deserialize_hex(&event.hex).expect("decode hex");
        assert_eq!(decoded, response.0);
    }
}
