use crate::clients::rpc_client;
use crate::extractors::extractor_trait::{Extractor, ExtractorError};
use corepc_client::types::model::GetRawMempoolVerbose;
use std::fmt::Debug;

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

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use corepc_client::bitcoin::hashes::Hash;
    use corepc_client::bitcoin::{Amount, Txid, Wtxid};
    use corepc_client::types::model::{MempoolEntry, MempoolEntryFees};

    fn dummy_response(seeds: &[[u8; 32]]) -> GetRawMempoolVerbose {
        let mut map = BTreeMap::new();
        for seed in seeds {
            map.insert(Txid::from_byte_array(*seed), dummy_mempool_entry());
        }
        GetRawMempoolVerbose(map)
    }

    fn dummy_mempool_entry() -> MempoolEntry {
        MempoolEntry {
            vsize: None,
            size: None,
            weight: None,
            time: 0,
            height: 0,
            descendant_count: 0,
            descendant_size: 0,
            ancestor_count: 0,
            ancestor_size: 0,
            wtxid: Wtxid::from_byte_array([0u8; 32]),
            fees: MempoolEntryFees {
                base: Amount::from_sat(0),
                modified: Amount::from_sat(0),
                ancestor: Amount::from_sat(0),
                descendant: Amount::from_sat(0),
            },
            depends: vec![],
            spent_by: vec![],
            bip125_replaceable: None,
            unbroadcast: None,
        }
    }

    #[test]
    fn getrawmempool_should_map_mempool_txids_into_event() {
        let mut extractor = GetRawMempoolExtractor;
        let response = dummy_response(&[[1u8; 32], [2u8; 32]]);

        let event = extractor.into_event(&response);

        let expected: Vec<String> = vec![
            Txid::from_byte_array([1u8; 32]).to_string(),
            Txid::from_byte_array([2u8; 32]).to_string(),
        ];
        assert_eq!(event.txids, expected);
    }

    #[test]
    fn getrawmempool_should_yield_no_txids_on_empty_mempool() {
        let mut extractor = GetRawMempoolExtractor;
        let response = dummy_response(&[]);

        let event = extractor.into_event(&response);

        assert!(event.txids.is_empty());
    }

    #[test]
    fn getrawmempool_should_always_allow_extracting_again() {
        let extractor = GetRawMempoolExtractor;
        assert!(extractor.can_extract_again());
    }

    #[test]
    fn getrawmempool_should_report_change_on_update_last_response() {
        let mut extractor = GetRawMempoolExtractor;
        let response = dummy_response(&[[3u8; 32]]);
        assert!(extractor.update_last_response(&response));
    }

    #[test]
    fn getrawmempool_should_show_txids_len_in_debug() {
        let mut extractor = GetRawMempoolExtractor;
        let event = extractor.into_event(&dummy_response(&[[1u8; 32], [2u8; 32]]));
        assert_eq!(format!("{event:?}"), "GetRawMempoolEvent { txids len: 2 }");
    }
}
