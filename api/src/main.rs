#![cfg_attr(feature = "strict", deny(warnings))]

use api::controllers::cluster::ClusterControllerRouter;
use api::controllers::mempool::MempoolControllerRouter;
use api::db;
use api::infra::config::{ApiConfig, CONFIG_PATH};
use api::infra::state::AppState;
use axum::{Router, routing::get};
use std::path::Path;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    let cfg = ApiConfig::load(Path::new(CONFIG_PATH)).expect("failed to load config");
    shared::logging::init_tracing(&cfg.observer.log_level);

    db::run_migrations(&cfg.database.url).expect("failed to run migrations");
    let db_pool = db::build_pool(&cfg.database.url).expect("failed to build db pool");

    let (state, snapshot, clients) = AppState::build(&cfg.observer, db_pool);

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .add_mempool_routes()
        .add_cluster_routes()
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .expect("failed to bind listener");

    tracing::info!("api listening on {}", cfg.bind);

    state.bootstrap_service.run(&cfg, clients, snapshot).await;

    axum::serve(listener, app).await.expect("server error");
}
