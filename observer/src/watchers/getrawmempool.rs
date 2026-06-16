use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use crate::infra::config::WatchersConfig;
use crate::watchers::watcher_trait::Watcher;
use async_nats::Client;
use corepc_client::types::model::GetRawMempool;
use shared::events::GetRawMempoolEvent;
use shared::subjects::Subject;
use std::collections::HashSet;

pub struct GetRawMempoolWatcher {
    nats: Client,
    rpc: RpcClient,
    delta: MempoolDelta,
}

impl GetRawMempoolWatcher {
    pub fn new(nats: Client, rpc: RpcClient) -> Self {
        Self {
            nats,
            rpc,
            delta: MempoolDelta::default(),
        }
    }
}

impl Watcher for GetRawMempoolWatcher {
    type Response = GetRawMempool;
    type Event = GetRawMempoolEvent;

    fn publisher(&self) -> &Client {
        &self.nats
    }

    fn subject(&self) -> Subject {
        Subject::RawMempool
    }

    async fn watch(&mut self) -> Result<Self::Response, ObserverError> {
        let response = self.rpc.call(|client| client.get_raw_mempool()).await?;

        response
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))
    }

    fn can_watch_again(&self) -> bool {
        true
    }

    fn update_last_response(&mut self, response: &Self::Response) -> bool {
        self.delta.update(response)
    }

    fn to_event(&self, _response: &Self::Response) -> Self::Event {
        self.delta.to_event()
    }

    fn is_enabled(&self, config: &WatchersConfig) -> bool {
        config.getrawmempool
    }
}

/// Tracks the mempool between polls and exposes the added/removed delta.
#[derive(Default)]
struct MempoolDelta {
    last_txids: HashSet<String>,
    last_added: Vec<String>,
    last_removed: Vec<String>,
}

impl MempoolDelta {
    /// Advances the tracked set to `response`. Returns true if it changed.
    fn update(&mut self, response: &GetRawMempool) -> bool {
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

    fn to_event(&self) -> GetRawMempoolEvent {
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
    fn poll(delta: &mut MempoolDelta, seeds: &[[u8; 32]]) -> GetRawMempoolEvent {
        delta.update(&dummy_response(seeds));
        delta.to_event()
    }

    #[test]
    fn getrawmempool_should_report_no_change_when_mempool_repeats() {
        let mut delta = MempoolDelta::default();

        // {A}: changed
        assert!(delta.update(&dummy_response(&[[1u8; 32]])));
        // {A}: no change
        assert!(!delta.update(&dummy_response(&[[1u8; 32]])));
        // {A, B}: changed
        assert!(delta.update(&dummy_response(&[[1u8; 32], [2u8; 32]])));
    }

    #[test]
    fn getrawmempool_should_report_all_txids_as_added_on_first_event() {
        let mut delta = MempoolDelta::default();

        let event = poll(&mut delta, &[[1u8; 32], [2u8; 32]]);

        let mut expected = vec![txid_str([1u8; 32]), txid_str([2u8; 32])];
        expected.sort();
        assert_eq!(event.added, expected);
        assert!(event.removed.is_empty());
    }

    #[test]
    fn getrawmempool_should_yield_empty_delta_on_no_change() {
        let mut delta = MempoolDelta::default();

        poll(&mut delta, &[[1u8; 32], [2u8; 32]]); // baseline {A, B}
        let event = poll(&mut delta, &[[1u8; 32], [2u8; 32]]); // no change

        assert!(event.added.is_empty());
        assert!(event.removed.is_empty());
    }

    #[test]
    fn getrawmempool_should_report_added_txids_between_polls() {
        let mut delta = MempoolDelta::default();
        poll(&mut delta, &[[1u8; 32]]); // baseline {A}

        let event = poll(&mut delta, &[[1u8; 32], [2u8; 32]]);

        assert_eq!(event.added, vec![txid_str([2u8; 32])]);
        assert!(event.removed.is_empty());
    }

    #[test]
    fn getrawmempool_should_report_removed_txids_between_polls() {
        let mut delta = MempoolDelta::default();
        poll(&mut delta, &[[1u8; 32], [2u8; 32]]); // baseline {A, B}

        let event = poll(&mut delta, &[[1u8; 32]]);

        assert!(event.added.is_empty());
        assert_eq!(event.removed, vec![txid_str([2u8; 32])]);
    }

    #[test]
    fn getrawmempool_should_report_both_added_and_removed_on_turnover() {
        let mut delta = MempoolDelta::default();
        poll(&mut delta, &[[1u8; 32]]); // baseline {A}

        let event = poll(&mut delta, &[[2u8; 32]]); // A out, B in

        assert_eq!(event.added, vec![txid_str([2u8; 32])]);
        assert_eq!(event.removed, vec![txid_str([1u8; 32])]);
    }
}
