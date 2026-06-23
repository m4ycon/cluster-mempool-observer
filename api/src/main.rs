#![cfg_attr(feature = "strict", deny(warnings))]

use api::controllers::mempool::MempoolControllerRouter;
use api::db;
use api::infra::config::{ApiConfig, CONFIG_PATH};
use api::infra::state::AppState;
use api::services::bootstrap::bootstrap;
use api::services::mempool;
use axum::{Router, routing::get};
use observer::clients::{Clients, rpc_client::RpcClient};
use shared::pubsub::PubSub;
use std::path::Path;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    let cfg = ApiConfig::load(Path::new(CONFIG_PATH)).expect("failed to load config");
    shared::logging::init_tracing(&cfg.observer.log_level);

    db::run_migrations(&cfg.database.url).expect("failed to run migrations");
    let db_pool = db::build_pool(&cfg.database.url).expect("failed to build db pool");

    let clients = Clients {
        pubsub: PubSub::new(),
        rpc: RpcClient::new(&cfg.observer.rpc).expect("failed to initialize RPC client"),
    };
    let (state, snapshot) = AppState::build(clients.clone(), db_pool);

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .add_mempool_routes()
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .expect("failed to bind listener");

    tracing::info!("api listening on {}", cfg.bind);

    bootstrap(&state, &snapshot).await;

    let delta_stream = mempool::mempool_delta_stream(&state.pubsub).await;
    tokio::spawn(mempool::persist_deltas_and_new_txs(
        state.mempool_delta_repository.clone(),
        state.transaction_repository.clone(),
        state.transaction_retriever.clone(),
        delta_stream,
    ));

    let observer_cfg = cfg.observer.clone();
    tokio::spawn(async move { observer::runner::run(&observer_cfg, clients, snapshot).await });

    axum::serve(listener, app).await.expect("server error");
}
