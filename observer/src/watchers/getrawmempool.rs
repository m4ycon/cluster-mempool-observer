use crate::clients::rpc_client;
use crate::error::ObserverError;
use crate::infra::config::WatchersConfig;
use crate::watchers::watcher_trait::Watcher;
use corepc_client::types::model::GetRawMempool;
use shared::events::GetRawMempoolEvent;
use shared::subjects::Subject;
use std::collections::HashSet;

#[derive(Default)]
pub struct GetRawMempoolWatcher {
    last_txids: HashSet<String>,
    last_added: Vec<String>,
    last_removed: Vec<String>,
}

impl Watcher for GetRawMempoolWatcher {
    type Response = GetRawMempool;
    type Event = GetRawMempoolEvent;

    fn subject(&self) -> Subject {
        Subject::RawMempool
    }

    async fn watch(&mut self) -> Result<Self::Response, ObserverError> {
        let response = rpc_client::get()
            .call(|client| client.get_raw_mempool())
            .await?;

        response
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))
    }

    fn can_watch_again(&self) -> bool {
        true
    }

    fn update_last_response(&mut self, response: &Self::Response) -> bool {
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

    fn to_event(&self, _response: &Self::Response) -> Self::Event {
        Self::Event {
            added: self.last_added.clone(),
            removed: self.last_removed.clone(),
        }
    }

    fn is_enabled(&self, config: &WatchersConfig) -> bool {
        config.getrawmempool
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
    fn poll(watcher: &mut GetRawMempoolWatcher, seeds: &[[u8; 32]]) -> GetRawMempoolEvent {
        let response = dummy_response(seeds);
        watcher.update_last_response(&response);
        watcher.to_event(&response)
    }

    #[test]
    fn getrawmempool_should_always_allow_extracting_again() {
        let watcher = GetRawMempoolWatcher::default();
        assert!(watcher.can_watch_again());
    }

    #[test]
    fn getrawmempool_should_report_no_change_when_mempool_repeats() {
        let mut watcher = GetRawMempoolWatcher::default();

        // {A}: changed
        assert!(watcher.update_last_response(&dummy_response(&[[1u8; 32]])));
        // {A}: no change
        assert!(!watcher.update_last_response(&dummy_response(&[[1u8; 32]])));
        // {A, B}: changed
        assert!(watcher.update_last_response(&dummy_response(&[[1u8; 32], [2u8; 32]])));
    }

    #[test]
    fn getrawmempool_should_report_all_txids_as_added_on_first_event() {
        let mut watcher = GetRawMempoolWatcher::default();

        let event = poll(&mut watcher, &[[1u8; 32], [2u8; 32]]);

        let mut expected = vec![txid_str([1u8; 32]), txid_str([2u8; 32])];
        expected.sort();
        assert_eq!(event.added, expected);
        assert!(event.removed.is_empty());
    }

    #[test]
    fn getrawmempool_should_yield_empty_delta_on_no_change() {
        let mut watcher = GetRawMempoolWatcher::default();

        poll(&mut watcher, &[[1u8; 32], [2u8; 32]]); // baseline {A, B}
        let event = poll(&mut watcher, &[[1u8; 32], [2u8; 32]]); // no change

        assert!(event.added.is_empty());
        assert!(event.removed.is_empty());
    }

    #[test]
    fn getrawmempool_should_report_added_txids_between_polls() {
        let mut watcher = GetRawMempoolWatcher::default();
        poll(&mut watcher, &[[1u8; 32]]); // baseline {A}

        let event = poll(&mut watcher, &[[1u8; 32], [2u8; 32]]);

        assert_eq!(event.added, vec![txid_str([2u8; 32])]);
        assert!(event.removed.is_empty());
    }

    #[test]
    fn getrawmempool_should_report_removed_txids_between_polls() {
        let mut watcher = GetRawMempoolWatcher::default();
        poll(&mut watcher, &[[1u8; 32], [2u8; 32]]); // baseline {A, B}

        let event = poll(&mut watcher, &[[1u8; 32]]);

        assert!(event.added.is_empty());
        assert_eq!(event.removed, vec![txid_str([2u8; 32])]);
    }

    #[test]
    fn getrawmempool_should_report_both_added_and_removed_on_turnover() {
        let mut watcher = GetRawMempoolWatcher::default();
        poll(&mut watcher, &[[1u8; 32]]); // baseline {A}

        let event = poll(&mut watcher, &[[2u8; 32]]); // A out, B in

        assert_eq!(event.added, vec![txid_str([2u8; 32])]);
        assert_eq!(event.removed, vec![txid_str([1u8; 32])]);
    }
}
