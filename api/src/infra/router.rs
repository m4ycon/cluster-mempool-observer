use crate::controllers::cluster::ClusterControllerRouter;
use crate::controllers::mempool::MempoolControllerRouter;
use crate::infra::metrics;
use crate::infra::state::AppState;
use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;

/// Builds the public API router
pub fn build(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .add_mempool_routes()
        .add_cluster_routes()
        .layer(metrics::http_layer())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
