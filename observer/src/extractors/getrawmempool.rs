use crate::clients::rpc_client;
use crate::extractors::extractor_trait::{Extractor, ExtractorError};
use corepc_client::types::model::GetRawMempool;
use std::collections::HashSet;
use std::fmt::Debug;
use std::time::Instant;

#[derive(Default)]
pub struct GetRawMempoolExtractor {
    last_txids: HashSet<String>,
    last_added: Vec<String>,
    last_removed: Vec<String>,
}

/// A delta of the get_raw_mempool between two consecutive polls
pub struct GetRawMempoolEvent {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl Debug for GetRawMempoolEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetRawMempoolEvent {{ added: {}, removed: {} }}",
            self.added.len(),
            self.removed.len()
        )
    }
}

impl Extractor<GetRawMempool, GetRawMempoolEvent> for GetRawMempoolExtractor {
    async fn extract(&mut self) -> Result<GetRawMempool, ExtractorError> {
        let start = Instant::now();
        let response = rpc_client::get()
            .call(|client| client.get_raw_mempool())
            .await?;
        tracing::debug!("get_raw_mempool RPC call took {:?}", start.elapsed());

        response
            .into_model()
            .map_err(|e| ExtractorError::FailedToExtract(e.to_string()))
    }

    fn can_extract_again(&self) -> bool {
        true
    }

    fn update_last_response(&mut self, response: &GetRawMempool) -> bool {
        let new_txids: HashSet<String> = response.0.iter().map(|txid| txid.to_string()).collect();
        if new_txids == self.last_txids {
            self.last_added = vec![];
            self.last_removed = vec![];
            return false;
        }

        let mut added: Vec<String> = new_txids.difference(&self.last_txids).cloned().collect();
        let mut removed: Vec<String> = self.last_txids.difference(&new_txids).cloned().collect();
        added.sort();
        removed.sort();

        self.last_txids = new_txids;
        self.last_added = added;
        self.last_removed = removed;
        true
    }

    fn into_event(&mut self, _response: &GetRawMempool) -> GetRawMempoolEvent {
        GetRawMempoolEvent {
            added: self.last_added.clone(),
            removed: self.last_removed.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use corepc_client::bitcoin::Txid;
    use corepc_client::bitcoin::hashes::Hash;

    fn dummy_response(seeds: &[[u8; 32]]) -> GetRawMempool {
        let txids = seeds
            .iter()
            .map(|seed| Txid::from_byte_array(*seed))
            .collect();
        GetRawMempool(txids)
    }

    fn txid_str(seed: [u8; 32]) -> String {
        Txid::from_byte_array(seed).to_string()
    }

    /// Mirrors the runner: advance state, then build the event from the delta.
    fn poll(extractor: &mut GetRawMempoolExtractor, seeds: &[[u8; 32]]) -> GetRawMempoolEvent {
        let response = dummy_response(seeds);
        extractor.update_last_response(&response);
        extractor.into_event(&response)
    }

    #[test]
    fn getrawmempool_should_always_allow_extracting_again() {
        let extractor = GetRawMempoolExtractor::default();
        assert!(extractor.can_extract_again());
    }

    #[test]
    fn getrawmempool_should_report_no_change_when_mempool_repeats() {
        let mut extractor = GetRawMempoolExtractor::default();

        // {A}: changed
        assert!(extractor.update_last_response(&dummy_response(&[[1u8; 32]])));
        // {A}: no change
        assert!(!extractor.update_last_response(&dummy_response(&[[1u8; 32]])));
        // {A, B}: changed
        assert!(extractor.update_last_response(&dummy_response(&[[1u8; 32], [2u8; 32]])));
    }

    #[test]
    fn getrawmempool_should_report_all_txids_as_added_on_first_event() {
        let mut extractor = GetRawMempoolExtractor::default();

        let event = poll(&mut extractor, &[[1u8; 32], [2u8; 32]]);

        let mut expected = vec![txid_str([1u8; 32]), txid_str([2u8; 32])];
        expected.sort();
        assert_eq!(event.added, expected);
        assert!(event.removed.is_empty());
    }

    #[test]
    fn getrawmempool_should_yield_empty_delta_on_no_change() {
        let mut extractor = GetRawMempoolExtractor::default();

        poll(&mut extractor, &[[1u8; 32], [2u8; 32]]); // baseline {A, B}
        let event = poll(&mut extractor, &[[1u8; 32], [2u8; 32]]); // no change

        assert!(event.added.is_empty());
        assert!(event.removed.is_empty());
    }

    #[test]
    fn getrawmempool_should_report_added_txids_between_polls() {
        let mut extractor = GetRawMempoolExtractor::default();
        poll(&mut extractor, &[[1u8; 32]]); // baseline {A}

        let event = poll(&mut extractor, &[[1u8; 32], [2u8; 32]]);

        assert_eq!(event.added, vec![txid_str([2u8; 32])]);
        assert!(event.removed.is_empty());
    }

    #[test]
    fn getrawmempool_should_report_removed_txids_between_polls() {
        let mut extractor = GetRawMempoolExtractor::default();
        poll(&mut extractor, &[[1u8; 32], [2u8; 32]]); // baseline {A, B}

        let event = poll(&mut extractor, &[[1u8; 32]]);

        assert!(event.added.is_empty());
        assert_eq!(event.removed, vec![txid_str([2u8; 32])]);
    }

    #[test]
    fn getrawmempool_should_report_both_added_and_removed_on_turnover() {
        let mut extractor = GetRawMempoolExtractor::default();
        poll(&mut extractor, &[[1u8; 32]]); // baseline {A}

        let event = poll(&mut extractor, &[[2u8; 32]]); // A out, B in

        assert_eq!(event.added, vec![txid_str([2u8; 32])]);
        assert_eq!(event.removed, vec![txid_str([1u8; 32])]);
    }
}
