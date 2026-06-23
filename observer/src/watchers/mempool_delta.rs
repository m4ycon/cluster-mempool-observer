use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use crate::infra::config::WatchersConfig;
use crate::snapshot::MempoolSnapshot;
use crate::watchers::watcher_trait::Watcher;
use corepc_client::types::model::GetRawMempool;
use shared::events::MempoolDeltaEvent;
use shared::subjects::Subject;
use std::collections::HashSet;

pub struct MempoolDeltaWatcher {
    rpc: RpcClient,
    watch_rate: u32,
    delta: MempoolDelta,
    snapshot: MempoolSnapshot,
}

impl MempoolDeltaWatcher {
    pub fn new(rpc: RpcClient, watch_rate: u32, snapshot: MempoolSnapshot) -> Self {
        let delta = MempoolDelta {
            last_txids: snapshot.get(), // seeded by bootstrap
            ..Default::default()
        };
        Self {
            rpc,
            watch_rate,
            delta,
            snapshot,
        }
    }

    /// Returns a clone of the currently tracked mempool txid set.
    pub fn txids(&self) -> HashSet<String> {
        self.snapshot.get()
    }
}

impl Watcher for MempoolDeltaWatcher {
    type Response = GetRawMempool;
    type Event = MempoolDeltaEvent;

    async fn watch(&mut self) -> Result<Option<Self::Response>, ObserverError> {
        let response = self
            .rpc
            .call(|client| client.get_raw_mempool())
            .await?
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))?;

        let changed = self.delta.update(&response);
        self.snapshot.store(self.delta.last_txids.clone());
        Ok(changed.then_some(response))
    }

    fn to_event(&self, _response: &Self::Response) -> Self::Event {
        self.delta.to_event()
    }

    fn get_watch_rate(&self) -> u32 {
        self.watch_rate
    }

    fn get_publish_subject(&self) -> Subject {
        Subject::MempoolDelta
    }

    fn is_enabled(&self, config: &WatchersConfig) -> bool {
        config.mempool_delta
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

    fn to_event(&self) -> MempoolDeltaEvent {
        MempoolDeltaEvent {
            added: self.last_added.clone(),
            removed: self.last_removed.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::infra::config::RpcConfig;
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
    fn poll(delta: &mut MempoolDelta, seeds: &[[u8; 32]]) -> MempoolDeltaEvent {
        delta.update(&dummy_response(seeds));
        delta.to_event()
    }

    fn dummy_rpc() -> RpcClient {
        RpcClient::new(&RpcConfig {
            host: "127.0.0.1:18443".into(),
            user: "user".into(),
            pass: "pass".into(),
        })
        .expect("build rpc client")
    }

    #[test]
    fn watcher_seeds_baseline_from_snapshot_so_unchanged_mempool_yields_no_delta() {
        let seeds = [[1u8; 32], [2u8; 32]];
        let seeded: HashSet<String> = seeds.iter().map(|s| txid_str(*s)).collect();

        let snapshot = MempoolSnapshot::default();
        snapshot.store(seeded.clone());

        let mut watcher = MempoolDeltaWatcher::new(dummy_rpc(), 1, snapshot);

        // baseline must match the snapshot, not start empty
        assert_eq!(watcher.delta.last_txids, seeded);
        // first poll returning the same set reports no change
        assert!(!watcher.delta.update(&dummy_response(&seeds)));
    }

    #[test]
    fn mempool_delta_should_report_no_change_when_mempool_repeats() {
        let mut delta = MempoolDelta::default();

        // {A}: changed
        assert!(delta.update(&dummy_response(&[[1u8; 32]])));
        // {A}: no change
        assert!(!delta.update(&dummy_response(&[[1u8; 32]])));
        // {A, B}: changed
        assert!(delta.update(&dummy_response(&[[1u8; 32], [2u8; 32]])));
    }

    #[test]
    fn mempool_delta_should_report_all_txids_as_added_on_first_event() {
        let mut delta = MempoolDelta::default();

        let event = poll(&mut delta, &[[1u8; 32], [2u8; 32]]);

        let mut expected = vec![txid_str([1u8; 32]), txid_str([2u8; 32])];
        expected.sort();
        assert_eq!(event.added, expected);
        assert!(event.removed.is_empty());
    }

    #[test]
    fn mempool_delta_should_yield_empty_delta_on_no_change() {
        let mut delta = MempoolDelta::default();

        poll(&mut delta, &[[1u8; 32], [2u8; 32]]); // baseline {A, B}
        let event = poll(&mut delta, &[[1u8; 32], [2u8; 32]]); // no change

        assert!(event.added.is_empty());
        assert!(event.removed.is_empty());
    }

    #[test]
    fn mempool_delta_should_report_added_txids_between_polls() {
        let mut delta = MempoolDelta::default();
        poll(&mut delta, &[[1u8; 32]]); // baseline {A}

        let event = poll(&mut delta, &[[1u8; 32], [2u8; 32]]);

        assert_eq!(event.added, vec![txid_str([2u8; 32])]);
        assert!(event.removed.is_empty());
    }

    #[test]
    fn mempool_delta_should_report_removed_txids_between_polls() {
        let mut delta = MempoolDelta::default();
        poll(&mut delta, &[[1u8; 32], [2u8; 32]]); // baseline {A, B}

        let event = poll(&mut delta, &[[1u8; 32]]);

        assert!(event.added.is_empty());
        assert_eq!(event.removed, vec![txid_str([2u8; 32])]);
    }

    #[test]
    fn mempool_delta_should_report_both_added_and_removed_on_turnover() {
        let mut delta = MempoolDelta::default();
        poll(&mut delta, &[[1u8; 32]]); // baseline {A}

        let event = poll(&mut delta, &[[2u8; 32]]); // A out, B in

        assert_eq!(event.added, vec![txid_str([2u8; 32])]);
        assert_eq!(event.removed, vec![txid_str([1u8; 32])]);
    }
}
