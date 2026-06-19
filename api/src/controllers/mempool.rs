use crate::controllers::websocket;
use crate::infra::state::AppRouter;
use crate::services::{mempool, pubsub::PubSubService};
use axum::Json;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;
use axum::routing::get;
use futures::StreamExt;
use futures::stream;
use observer::retrievers::MempoolRetriever;
use shared::events::MempoolDeltaEvent;
use shared::subjects::Subject;
use std::collections::HashSet;

pub trait MempoolControllerRouter {
    fn add_mempool_routes(self) -> Self;
}

impl MempoolControllerRouter for AppRouter {
    fn add_mempool_routes(self) -> Self {
        self.route("/mempool/delta", get(mempool_delta_ws))
            .route("/mempool/txids", get(mempool_txids))
    }
}

/// Returns the watcher's current mempool txid set.
async fn mempool_txids(State(retriever): State<MempoolRetriever>) -> Json<HashSet<String>> {
    Json(retriever.mempool_txids())
}

/// Upgrades the connection to a websocket that streams `MempoolDeltaEvent`s.
async fn mempool_delta_ws(
    ws: WebSocketUpgrade,
    State(pubsub): State<PubSubService>,
    State(retriever): State<MempoolRetriever>,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        // Subscribe to the mempool delta stream first to avoid data gaps
        let delta_stream = mempool::mempool_delta_stream(&pubsub).await;

        let snapshot = MempoolDeltaEvent {
            added: retriever.mempool_txids().into_iter().collect(),
            removed: Vec::new(),
        };

        let stream = stream::once(async move { snapshot }).chain(delta_stream);

        tracing::info!("Websocket client subscribed to {}", Subject::MempoolDelta);

        websocket::stream(socket, stream).await;
    })
}
