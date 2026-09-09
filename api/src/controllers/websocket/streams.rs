use crate::infra::state::AppState;
use futures::StreamExt;
use futures::stream::{self, BoxStream};
use shared::ws::{ServerEvent, WsSubject};
use std::time::Duration;
use tokio_stream::wrappers::IntervalStream;

const MEMPOOL_STATS_INTERVAL: Duration = Duration::from_secs(5);

/// Builds the event stream for one subject. Called from inside a spawned
/// subscription task, so hitting pubsub or the DB here never stalls the
/// connection's read loop.
pub async fn build(subject: WsSubject, state: &AppState) -> BoxStream<'static, ServerEvent> {
    match subject {
        WsSubject::ClusterDelta => {
            // Subscribe to the change stream first to avoid data gaps
            let delta_stream = state.cluster_service.get_delta_stream().await;
            let snapshot = state.cluster_service.get_current_snapshot();

            stream::iter(snapshot)
                .chain(delta_stream)
                .map(ServerEvent::ClusterDelta)
                .boxed()
        }
        WsSubject::MempoolDelta => {
            let mempool_ledger = state.mempool_ledger.clone();
            let delta_stream = state
                .mempool_service
                .get_snapshot_then_delta_stream(
                    || async move { mempool_ledger.clone_live_snapshot() },
                )
                .await;

            delta_stream.map(ServerEvent::MempoolDelta).boxed()
        }
        WsSubject::MempoolStats => {
            let home = state.home_service.clone();

            IntervalStream::new(tokio::time::interval(MEMPOOL_STATS_INTERVAL))
                .then(move |_| {
                    let home = home.clone();
                    async move { home.current_stats().await }
                })
                .map(ServerEvent::MempoolStats)
                .boxed()
        }
        WsSubject::ChainTip => {
            let home = state.home_service.clone();
            let new_blocks = home.new_block_stream().await;
            let initial = home.get_current_chain_tip().await;

            stream::iter(initial)
                .chain(new_blocks)
                .map(ServerEvent::ChainTip)
                .boxed()
        }
    }
}
