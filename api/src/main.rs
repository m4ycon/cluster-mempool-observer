#![cfg_attr(feature = "strict", deny(warnings))]

use crate::controllers::mempool::MempoolControllerRouter;
use axum::{Router, routing::get};
use infra::config::{ApiConfig, CONFIG_PATH};
use infra::state::AppState;
use observer::clients::{Clients, rpc_client::RpcClient};
use services::pubsub::PubSubService;
use shared::pubsub::PubSub;
use std::path::Path;

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

    let observer_cfg = cfg.observer.clone();
    let clients = Clients {
        pubsub: pubsub.clone(),
        rpc,
    };
    tokio::spawn(async move { observer::runner::run(&observer_cfg, clients).await });

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .add_mempool_routes()
        .with_state(AppState::new(PubSubService::new(pubsub)));

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .expect("failed to bind listener");

    tracing::info!("api listening on {}", cfg.bind);

    axum::serve(listener, app).await.expect("server error");
}
