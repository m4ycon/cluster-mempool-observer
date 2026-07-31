use crate::db::{BlockRepository, MempoolDeltaRepository};
use crate::services::cluster::ClusterService;
use crate::services::pubsub::PubSubService;
use futures::Stream;
use observer::retrievers::{ClusterRetriever, ClusterRpcRetriever, MempoolRetriever};
use shared::events::{MempoolStatsEvent, NewBlockInfoEvent};
use shared::metrics::timed_async;
use shared::subjects::Subject;
use time::{Duration, OffsetDateTime};

/// Rolling window for the arrival-rate metric (txs/min).
const RATE_WINDOW: Duration = Duration::seconds(60);

/// Time to recompute the stats frame.
const STATS_SECONDS: &str = "home_stats_seconds";

#[derive(Clone)]
pub struct HomeService<CR: ClusterRetriever = ClusterRpcRetriever> {
    block_repository: BlockRepository,
    mempool_delta_repository: MempoolDeltaRepository,
    mempool_retriever: MempoolRetriever,
    cluster_service: ClusterService<CR>,
    pubsub: PubSubService,
}

impl<CR: ClusterRetriever> HomeService<CR> {
    pub fn new(
        block_repository: BlockRepository,
        mempool_delta_repository: MempoolDeltaRepository,
        mempool_retriever: MempoolRetriever,
        cluster_service: ClusterService<CR>,
        pubsub: PubSubService,
    ) -> Self {
        Self {
            block_repository,
            mempool_delta_repository,
            mempool_retriever,
            cluster_service,
            pubsub,
        }
    }

    /// Current mempool counters, recomputed from live state.
    pub async fn current_stats(&self) -> MempoolStatsEvent {
        timed_async(STATS_SECONDS, self.current_stats_inner()).await
    }

    async fn current_stats_inner(&self) -> MempoolStatsEvent {
        let mempool_size = self.mempool_retriever.mempool_txids().len() as i64;
        let cluster_count = self.cluster_service.active_cluster_count() as i64;

        let cutoff = OffsetDateTime::now_utc() - RATE_WINDOW;
        let tx_per_min = match self.mempool_delta_repository.count_adds_since(cutoff).await {
            Ok(count) => count,
            Err(e) => {
                tracing::warn!("home stats: failed to count recent adds: {e}");
                0
            }
        };

        MempoolStatsEvent {
            mempool_size,
            cluster_count,
            tx_per_min,
        }
    }

    pub async fn get_current_chain_tip(&self) -> Option<NewBlockInfoEvent> {
        match self.block_repository.latest().await {
            Ok(Some((height, mined_at))) => Some(NewBlockInfoEvent { height, mined_at }),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!("home stats: failed to read latest block: {e}");
                None
            }
        }
    }

    /// New chain tips as they land.
    pub async fn new_block_stream(&self) -> impl Stream<Item = NewBlockInfoEvent> + use<CR> {
        self.pubsub
            .subscribe::<NewBlockInfoEvent>(Subject::NewBlockInfo)
            .await
    }
}
