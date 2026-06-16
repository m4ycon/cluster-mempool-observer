#![cfg_attr(feature = "strict", deny(warnings))]

use observer::{
    clients::{Clients, rpc_client::RpcClient},
    infra::config::init_config,
    runner,
};
use shared::logging::init_tracing;
use shared::nats;

#[tokio::main]
async fn main() {
    let config = match init_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    init_tracing(&config.log_level);

    let nats = match nats::connect(&config.nats).await {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Failed to connect to NATS server: {e:?}");
            std::process::exit(1);
        }
    };

    let rpc = match RpcClient::new(&config.rpc) {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Failed to initialize RPC client: {e:?}");
            std::process::exit(1);
        }
    };

    tracing::info!("Starting mempool observer...");

    runner::run(&config, Clients { nats, rpc }).await;
}
