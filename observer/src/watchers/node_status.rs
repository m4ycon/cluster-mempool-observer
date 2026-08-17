use crate::error::ObserverError;
use crate::retrievers::ChainRetriever;
use crate::watchers::watcher_trait::{Watcher, WatcherRPC};
use shared::events::NodeStatusEvent;
use shared::subjects::Subject;

pub struct NodeStatusWatcher<C: ChainRetriever> {
    chain: C,
    watch_rate: u32,
    last_reachable: Option<bool>,
}

impl<C: ChainRetriever> NodeStatusWatcher<C> {
    pub fn new(chain: C, watch_rate: u32) -> Self {
        Self {
            chain,
            watch_rate,
            last_reachable: None,
        }
    }
}

impl<C: ChainRetriever> Watcher for NodeStatusWatcher<C> {
    type Event = NodeStatusEvent;

    fn get_publish_subject(&self) -> Subject {
        Subject::NodeStatus
    }
}

impl<C: ChainRetriever> WatcherRPC for NodeStatusWatcher<C> {
    type Response = NodeStatusEvent;

    async fn watch(&mut self) -> Result<Option<Self::Response>, ObserverError> {
        let (reachable, event) = match self.chain.get_blockchain_info().await {
            Ok(info) => (true, NodeStatusEvent::reachable(&info)),
            Err(e) => (false, NodeStatusEvent::unreachable(e)),
        };

        if self.last_reachable == Some(reachable) {
            return Ok(None);
        }
        self.last_reachable = Some(reachable);

        Ok(Some(event))
    }

    fn to_event(&self, response: &Self::Response) -> Self::Event {
        response.clone()
    }

    fn get_watch_rate(&self) -> u32 {
        self.watch_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::models::GetBlockchainInfoModel;
    use std::sync::{Arc, Mutex};

    /// Returns whatever `response` currently holds, success or failure.
    #[derive(Clone)]
    struct ScriptedChain {
        response: Arc<Mutex<Result<GetBlockchainInfoModel, String>>>,
    }

    impl ScriptedChain {
        fn ok(info: GetBlockchainInfoModel) -> Self {
            Self {
                response: Arc::new(Mutex::new(Ok(info))),
            }
        }

        fn failing(error: &str) -> Self {
            Self {
                response: Arc::new(Mutex::new(Err(error.to_string()))),
            }
        }

        fn set(&self, response: Result<GetBlockchainInfoModel, String>) {
            *self.response.lock().unwrap() = response;
        }
    }

    impl ChainRetriever for ScriptedChain {
        async fn get_blockchain_info(&self) -> Result<GetBlockchainInfoModel, ObserverError> {
            match &*self.response.lock().unwrap() {
                Ok(info) => Ok(info.clone()),
                Err(e) => Err(ObserverError::FailedToFetch(e.clone())),
            }
        }
    }

    fn info() -> GetBlockchainInfoModel {
        GetBlockchainInfoModel {
            blocks: 100,
            headers: 100,
            verification_progress: 1.0,
            initial_block_download: false,
        }
    }

    #[tokio::test]
    async fn a_failed_rpc_yields_an_unreachable_event_instead_of_an_error() {
        let mut watcher = NodeStatusWatcher::new(ScriptedChain::failing("connection refused"), 1);

        let result = watcher.watch().await;

        let response = result
            .expect("watch must never return Err")
            .expect("the first observation must publish, whichever way the RPC went");
        match response {
            NodeStatusEvent::Unreachable { error } => assert!(!error.is_empty()),
            NodeStatusEvent::Reachable { .. } => panic!("expected Unreachable"),
        }
    }

    #[tokio::test]
    async fn a_successful_rpc_yields_a_reachable_event() {
        let mut watcher = NodeStatusWatcher::new(ScriptedChain::ok(info()), 1);

        let response = watcher
            .watch()
            .await
            .expect("watch must never return Err")
            .expect("watch must always return Some");

        match response {
            NodeStatusEvent::Reachable {
                blocks,
                headers,
                initial_block_download,
                ..
            } => {
                assert_eq!(blocks, 100);
                assert_eq!(headers, 100);
                assert!(!initial_block_download);
            }
            NodeStatusEvent::Unreachable { .. } => panic!("expected Reachable"),
        }
    }

    #[tokio::test]
    async fn an_unchanged_status_publishes_only_once() {
        let mut watcher = NodeStatusWatcher::new(ScriptedChain::ok(info()), 1);

        assert!(watcher.watch().await.unwrap().is_some());
        assert!(watcher.watch().await.unwrap().is_none());
        assert!(watcher.watch().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_sustained_outage_publishes_only_once() {
        let mut watcher = NodeStatusWatcher::new(ScriptedChain::failing("down"), 1);

        assert!(watcher.watch().await.unwrap().is_some());
        assert!(watcher.watch().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn every_flip_publishes_again() {
        let chain = ScriptedChain::ok(info());
        let mut watcher = NodeStatusWatcher::new(chain.clone(), 1);

        assert!(watcher.watch().await.unwrap().is_some(), "first reachable");

        chain.set(Err("down".to_string()));
        assert!(watcher.watch().await.unwrap().is_some(), "went unreachable");
        assert!(
            watcher.watch().await.unwrap().is_none(),
            "still unreachable"
        );

        chain.set(Ok(info()));
        assert!(watcher.watch().await.unwrap().is_some(), "recovered");
    }

    #[tokio::test]
    async fn a_new_chain_tip_alone_does_not_republish() {
        let chain = ScriptedChain::ok(info());
        let mut watcher = NodeStatusWatcher::new(chain.clone(), 1);

        assert!(watcher.watch().await.unwrap().is_some());

        chain.set(Ok(GetBlockchainInfoModel {
            blocks: 101,
            headers: 101,
            ..info()
        }));

        assert!(watcher.watch().await.unwrap().is_none());
    }
}
