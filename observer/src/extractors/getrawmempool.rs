use std::fmt::Debug;

use corepc_client::types::model::GetRawMempoolVerbose;

use crate::clients::rpc_client;
use crate::extractors::extractor_trait::{Extractor, ExtractorError};

#[derive(Default)]
pub struct GetRawMempoolExtractor;

pub struct GetRawMempoolEvent {
    pub txids: Vec<String>,
}

impl Debug for GetRawMempoolEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetRawMempoolEvent {{ txids len: {:?} }}",
            self.txids.len()
        )
    }
}

impl Extractor<GetRawMempoolVerbose, GetRawMempoolEvent> for GetRawMempoolExtractor {
    async fn extract(&mut self) -> Result<GetRawMempoolVerbose, ExtractorError> {
        let response = rpc_client::get()
            .call(|client| client.get_raw_mempool_verbose())
            .await?;

        response
            .into_model()
            .map_err(|e| ExtractorError::FailedToExtract(e.to_string()))
    }

    fn can_extract_again(&self) -> bool {
        true
    }

    fn update_last_response(&mut self, _response: &GetRawMempoolVerbose) -> bool {
        true
    }

    fn into_event(&mut self, response: &GetRawMempoolVerbose) -> GetRawMempoolEvent {
        let txids = response.0.keys().map(|txid| txid.to_string()).collect();
        GetRawMempoolEvent { txids }
    }
}
