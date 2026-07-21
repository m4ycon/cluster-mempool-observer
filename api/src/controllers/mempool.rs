use crate::controllers::websocket;
use crate::infra::state::{AppHomeService, AppMempoolService, AppRouter};
use axum::Json;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;
use axum::routing::get;
use futures::{StreamExt, stream};
use observer::retrievers::MempoolRetriever;
use shared::events::HomeMessage;
use shared::subjects::Subject;
use std::collections::HashSet;
use std::time::Duration;
use tokio_stream::wrappers::IntervalStream;

const STATS_INTERVAL: Duration = Duration::from_secs(5);

pub trait MempoolControllerRouter {
    fn add_mempool_routes(self) -> Self;
}

impl MempoolControllerRouter for AppRouter {
    fn add_mempool_routes(self) -> Self {
        self.route("/mempool/delta", get(mempool_delta_ws))
            .route("/mempool/txids", get(mempool_txids))
            .route("/mempool/stats", get(mempool_stats_ws))
    }
}

/// Returns the watcher's current mempool txid set.
async fn mempool_txids(State(mempool_retriever): State<MempoolRetriever>) -> Json<HashSet<String>> {
    Json(mempool_retriever.mempool_txids())
}

/// Upgrades the connection to a websocket that streams `HomeMessage`s.
async fn mempool_stats_ws(ws: WebSocketUpgrade, State(home): State<AppHomeService>) -> Response {
    ws.on_upgrade(move |socket| async move {
        // Subscribe to new blocks first, then seed with the current tip.
        let new_blocks = home.new_block_stream().await;
        let initial = home.get_current_chain_tip().await;
        let block_stream = stream::iter(initial)
            .chain(new_blocks)
            .map(HomeMessage::Block);

        // Stats: first tick fires immediately, then every interval.
        let stats_stream =
            IntervalStream::new(tokio::time::interval(STATS_INTERVAL)).then(move |_| {
                let home = home.clone();
                async move { HomeMessage::Stats(home.current_stats().await) }
            });

        let stream = stream::select(block_stream, stats_stream);

        tracing::info!("Websocket client subscribed to mempool stats");

        websocket::stream(socket, stream).await;
    })
}

/// Upgrades the connection to a websocket that streams `MempoolDeltaEvent`s.
async fn mempool_delta_ws(
    ws: WebSocketUpgrade,
    State(mempool_service): State<AppMempoolService>,
    State(mempool_retriever): State<MempoolRetriever>,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        tracing::info!("Websocket client subscribed to {}", Subject::MempoolDelta);

        let stream = mempool_service
            .get_snapshot_then_delta_stream(|| async move { mempool_retriever.mempool_txids() })
            .await;

        websocket::stream(socket, stream).await;
    })
}
