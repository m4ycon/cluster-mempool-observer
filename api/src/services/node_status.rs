use crate::services::node_health::NodeHealthService;
use crate::services::pubsub::PubSubService;
use futures::{Stream, StreamExt};
use observer::retrievers::NetworkRetriever;
use shared::events::NodeStatusEvent;
use shared::models::GetBlockchainInfoModel;
use shared::subjects::Subject;

#[derive(Clone)]
pub struct NodeStatusService<N: NetworkRetriever> {
    pubsub: PubSubService,
    node_health: NodeHealthService<N>,
}

impl<N: NetworkRetriever> NodeStatusService<N> {
    pub fn new(pubsub: PubSubService, node_health: NodeHealthService<N>) -> Self {
        Self {
            pubsub,
            node_health,
        }
    }

    pub async fn get_status_stream(&self) -> impl Stream<Item = NodeStatusEvent> + use<N> {
        self.pubsub
            .subscribe::<NodeStatusEvent>(Subject::NodeStatus)
            .await
    }

    pub async fn consume<S>(&self, stream: S)
    where
        S: Stream<Item = NodeStatusEvent>,
    {
        let mut stream = std::pin::pin!(stream);
        while let Some(event) = stream.next().await {
            match event {
                NodeStatusEvent::Reachable {
                    blocks,
                    headers,
                    verification_progress,
                    initial_block_download,
                } => {
                    let info = GetBlockchainInfoModel {
                        blocks,
                        headers,
                        verification_progress,
                        initial_block_download,
                    };
                    self.node_health.observe_reachable(&info).await;
                }
                NodeStatusEvent::Unreachable { error } => {
                    self.node_health.observe_unreachable(error).await;
                }
            }
        }
    }
}
