#![cfg_attr(feature = "strict", deny(warnings))]

use api::controllers::mempool::MempoolControllerRouter;
use api::db;
use api::db::{MempoolDeltaRepository, TransactionRepository};
use api::infra::config::{ApiConfig, CONFIG_PATH};
use api::infra::state::AppState;
use api::services::bootstrap::bootstrap;
use api::services::mempool;
use api::services::pubsub::PubSubService;
use axum::{Router, routing::get};
use observer::clients::{Clients, rpc_client::RpcClient};
use observer::retrievers::{MempoolRetriever, TransactionRpcRetriever};
use observer::snapshot::MempoolSnapshot;
use shared::pubsub::PubSub;
use std::path::Path;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    let cfg = ApiConfig::load(Path::new(CONFIG_PATH)).expect("failed to load config");
    shared::logging::init_tracing(&cfg.observer.log_level);

    db::run_migrations(&cfg.database.url).expect("failed to run migrations");
    let db_pool = db::build_pool(&cfg.database.url).expect("failed to build db pool");
    let transaction_repository = TransactionRepository::new(db_pool.clone());
    let mempool_delta_repository = MempoolDeltaRepository::new(db_pool.clone());

    let pubsub = PubSub::new();
    let pubsub_service = PubSubService::new(pubsub.clone());

    let rpc = RpcClient::new(&cfg.observer.rpc).expect("failed to initialize RPC client");

    let snapshot = MempoolSnapshot::default();

    let mempool_retriever = MempoolRetriever::new(rpc.clone(), snapshot.clone());
    let transaction_retriever = TransactionRpcRetriever::new(rpc.clone());

    let observer_cfg = cfg.observer.clone();
    let clients = Clients {
        pubsub: pubsub.clone(),
        rpc,
    };

    let state = AppState::new(
        pubsub_service.clone(),
        mempool_retriever,
        transaction_retriever,
        transaction_repository,
        mempool_delta_repository,
    );
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .add_mempool_routes()
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .expect("failed to bind listener");

    tracing::info!("api listening on {}", cfg.bind);

    bootstrap(&state).await;

    let delta_stream = mempool::mempool_delta_stream(&pubsub_service).await;
    tokio::spawn(mempool::persist_deltas_and_new_txs(
        state.mempool_delta_repository.clone(),
        state.transaction_repository.clone(),
        state.transaction_retriever.clone(),
        delta_stream,
    ));

    tokio::spawn(async move { observer::runner::run(&observer_cfg, clients, snapshot).await });

    axum::serve(listener, app).await.expect("server error");
}
