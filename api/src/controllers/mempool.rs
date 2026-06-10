use crate::controllers::websocket;
use crate::infra::state::AppRouter;
use crate::services::{mempool, nats::NatsService};
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;
use axum::routing::get;
use shared::subjects::Subject;

pub trait MempoolControllerRouter {
    fn add_mempool_routes(self) -> Self;
}

impl MempoolControllerRouter for AppRouter {
    fn add_mempool_routes(self) -> Self {
        self.route("/ws/rawmempool", get(rawmempool_ws))
    }
}

/// Upgrades the connection to a websocket that streams `GetRawMempoolEvent`s.
async fn rawmempool_ws(ws: WebSocketUpgrade, State(nats): State<NatsService>) -> Response {
    ws.on_upgrade(move |socket| async move {
        let stream = match mempool::rawmempool_stream(&nats).await {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!("Failed to subscribe to {}: {e}", Subject::RawMempool);
                return;
            }
        };

        tracing::info!("Websocket client subscribed to {}", Subject::RawMempool);

        websocket::stream(socket, stream).await;
    })
}
