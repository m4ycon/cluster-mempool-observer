use crate::controllers::websocket;
use crate::infra::state::AppRouter;
use crate::services::{mempool, pubsub::PubSubService};
use axum::Json;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;
use axum::routing::get;
use observer::retrievers::MempoolRetriever;
use shared::subjects::Subject;
use std::collections::HashSet;

pub trait MempoolControllerRouter {
    fn add_mempool_routes(self) -> Self;
}

impl MempoolControllerRouter for AppRouter {
    fn add_mempool_routes(self) -> Self {
        self.route("/ws/rawmempool", get(rawmempool_ws))
            .route("/rawmempool/txids", get(rawmempool_txids))
    }
}

/// Returns the watcher's current mempool txid set.
async fn rawmempool_txids(State(retriever): State<MempoolRetriever>) -> Json<HashSet<String>> {
    Json(retriever.mempool_txids())
}

/// Upgrades the connection to a websocket that streams `MempoolDeltaEvent`s.
async fn rawmempool_ws(ws: WebSocketUpgrade, State(pubsub): State<PubSubService>) -> Response {
    ws.on_upgrade(move |socket| async move {
        let stream = mempool::mempool_delta_stream(&pubsub).await;

        tracing::info!("Websocket client subscribed to {}", Subject::MempoolDelta);

        websocket::stream(socket, stream).await;
    })
}
