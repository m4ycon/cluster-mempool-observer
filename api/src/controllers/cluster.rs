use crate::controllers::websocket;
use crate::infra::state::{AppClusterService, AppRouter};
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;
use axum::routing::get;
use futures::StreamExt;
use futures::stream;
use shared::subjects::Subject;

pub trait ClusterControllerRouter {
    fn add_cluster_routes(self) -> Self;
}

impl ClusterControllerRouter for AppRouter {
    fn add_cluster_routes(self) -> Self {
        self.route("/clusters/delta", get(cluster_deltas_ws))
    }
}

/// Upgrades the connection to a websocket that streams `ClusterDeltaEvent`s.
async fn cluster_deltas_ws(
    ws: WebSocketUpgrade,
    State(cluster_service): State<AppClusterService>,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        // Subscribe to the change stream first to avoid data gaps
        let delta_stream = cluster_service.get_delta_stream().await;

        let snapshot = cluster_service.get_current_snapshot();

        let stream = stream::once(async move { snapshot }).chain(delta_stream);

        tracing::info!("Websocket client subscribed to {}", Subject::ClusterDelta);

        websocket::stream(socket, stream).await;
    })
}
