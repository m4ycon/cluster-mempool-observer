#![cfg_attr(feature = "strict", deny(warnings))]

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
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .expect("failed to bind listener");

    tracing::info!("api listening on {}", cfg.bind);

    // sync any blocks missed while the api was down
    state.block_service.sync_missing_blocks().await;

    // bootstrap the mempool state
    state.bootstrap_service.run(&snapshot).await;

    // spawn the block stream persister
    let block_service = state.block_service.clone();
    let block_stream = block_service.get_block_stream().await;
    tokio::spawn(async move { block_service.persist_blocks_and_txs(block_stream).await });

    // spawn the delta stream persister
    let mempool_service = state.mempool_service.clone();
    let delta_stream = mempool_service.get_delta_stream().await;
    tokio::spawn(async move {
        mempool_service
            .persist_deltas_and_new_txs(delta_stream)
            .await
    });

    // spawn the observer runner
    let observer_cfg = cfg.observer.clone();
    tokio::spawn(async move { observer::runner::run(&observer_cfg, clients, snapshot).await });

    axum::serve(listener, app).await.expect("server error");
}
