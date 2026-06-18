#![cfg_attr(feature = "strict", deny(warnings))]

use crate::controllers::mempool::MempoolControllerRouter;
use crate::services::bootstrap::bootstrap;
use axum::{Router, routing::get};
use infra::config::{ApiConfig, CONFIG_PATH};
use infra::state::AppState;
use observer::clients::{Clients, rpc_client::RpcClient};
use observer::retrievers::{MempoolRetriever, TransactionRetriever};
use observer::snapshot::MempoolSnapshot;
use services::pubsub::PubSubService;
use shared::pubsub::PubSub;
use std::path::Path;
use tower_http::cors::CorsLayer;

mod controllers;
mod infra;
mod repositories;
mod services;

#[tokio::main]
async fn main() {
    let cfg = ApiConfig::load(Path::new(CONFIG_PATH)).expect("failed to load config");
    shared::logging::init_tracing(&cfg.observer.log_level);

    let pubsub = PubSub::new();

    let rpc = RpcClient::new(&cfg.observer.rpc).expect("failed to initialize RPC client");

    let snapshot = MempoolSnapshot::default();

    let mempool_retriever = MempoolRetriever::new(rpc.clone(), snapshot.clone());
    let transaction_retriever = TransactionRetriever::new(rpc.clone());

    let observer_cfg = cfg.observer.clone();
    let clients = Clients {
        pubsub: pubsub.clone(),
        rpc,
    };

    let state = AppState::new(
        PubSubService::new(pubsub.clone()),
        mempool_retriever.clone(),
        transaction_retriever.clone(),
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
    tokio::spawn(async move { observer::runner::run(&observer_cfg, clients, snapshot).await });

    axum::serve(listener, app).await.expect("server error");
}
