use crate::db::models::SystemEventKind;
use crate::services::system_event::SystemEventService;
use observer::error::ObserverError;
use observer::retrievers::NetworkRetriever;
use serde_json::json;
use shared::models::{GetBlockchainInfoModel, GetNetworkInfoModel};
use std::fmt::Display;
use std::future::Future;
use std::sync::{Arc, RwLock};

/// Last-known node reachability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reachability {
    Unknown,
    Reachable,
    Unreachable,
}

#[derive(Clone)]
pub struct NodeHealthService<N: NetworkRetriever> {
    system_event_service: SystemEventService,
    network_retriever: N,
    last: Arc<RwLock<Reachability>>,
}

impl<N: NetworkRetriever> NodeHealthService<N> {
    pub fn new(system_event_service: SystemEventService, network_retriever: N) -> Self {
        Self {
            system_event_service,
            network_retriever,
            last: Arc::new(RwLock::new(Reachability::Unknown)),
        }
    }

    /// Reports a successful `getblockchaininfo`. Node is reachable regardless of
    /// initial-block-download state.
    pub async fn observe_reachable(&self, info: &GetBlockchainInfoModel) {
        if !self.transition(Reachability::Reachable) {
            return;
        }

        let network_info = self.network_retriever.get_network_info().await.ok();

        self.system_event_service
            .record(
                SystemEventKind::NodeConnected,
                json!({
                    "blocks": info.blocks,
                    "headers": info.headers,
                    "subversion": network_info.as_ref().map(|i| &i.subversion),
                }),
            )
            .await;

        if let Some(network_info) = network_info {
            self.check_version(&network_info).await;
        }
    }

    /// Reports an RPC call that failed to reach the node.
    pub async fn observe_unreachable(&self, error: impl Display) {
        if !self.transition(Reachability::Unreachable) {
            return;
        }

        self.system_event_service
            .record(
                SystemEventKind::NodeDisconnected,
                json!({ "error": error.to_string() }),
            )
            .await;
    }

    /// Flips the last-known state and reports whether this observation changed it.
    fn transition(&self, observed: Reachability) -> bool {
        let mut last = self.last.write().unwrap_or_else(|e| e.into_inner());
        if *last == observed {
            return false;
        }
        *last = observed;
        true
    }

    /// Emits a `node_version_changed` row when we see a different subversion.
    async fn check_version(&self, network_info: &GetNetworkInfoModel) {
        let previous = match self
            .system_event_service
            .latest_of_kind(SystemEventKind::NodeVersionChanged)
            .await
        {
            Ok(previous) => previous,
            Err(e) => {
                tracing::warn!("node_health: failed to read latest node_version_changed: {e}");
                None
            }
        };

        let from_subversion = previous.and_then(|event| {
            event.details["to"]
                .as_str()
                .map(std::string::ToString::to_string)
        });

        if from_subversion.as_deref() == Some(network_info.subversion.as_str()) {
            return;
        }

        self.system_event_service
            .record(
                SystemEventKind::NodeVersionChanged,
                json!({
                    "from": from_subversion,
                    "to": network_info.subversion,
                    "version": network_info.version,
                }),
            )
            .await;
    }
}

/// Where node observations are reported. Kept as a trait so callers can
/// be tested without a database.
pub trait NodeHealthReporter: Clone + Send + Sync {
    fn observe_reachable(&self, info: &GetBlockchainInfoModel) -> impl Future<Output = ()> + Send;
    fn observe_unreachable(&self, error: &ObserverError) -> impl Future<Output = ()> + Send;
}

impl<N: NetworkRetriever> NodeHealthReporter for NodeHealthService<N> {
    async fn observe_reachable(&self, info: &GetBlockchainInfoModel) {
        NodeHealthService::observe_reachable(self, info).await;
    }

    async fn observe_unreachable(&self, error: &ObserverError) {
        NodeHealthService::observe_unreachable(self, error).await;
    }
}
