use crate::controllers::feerate_diagram::FeerateDiagramControllerRouter;
use crate::controllers::health::HealthControllerRouter;
use crate::controllers::mempool::MempoolControllerRouter;
use crate::controllers::snapshots::SnapshotsControllerRouter;
use crate::controllers::system_events::SystemEventsControllerRouter;
use crate::controllers::transactions::TransactionsControllerRouter;
use crate::controllers::websocket::WebsocketControllerRouter;
use crate::infra::metrics;
use crate::infra::readiness::require_ready;
use crate::infra::state::AppState;
use axum::{Router, middleware};
use tower_http::cors::CorsLayer;

/// Builds the public API router
pub fn build(state: AppState) -> Router {
    // Data routes are gated on readiness
    let data_routes = Router::new()
        .add_feerate_diagram_routes()
        .add_mempool_routes()
        .add_snapshot_routes()
        .add_transaction_routes()
        .add_websocket_routes()
        .route_layer(middleware::from_fn_with_state(state.clone(), require_ready));

    Router::new()
        .add_health_routes()
        .add_system_event_routes()
        .merge(data_routes)
        .layer(metrics::http_layer())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
