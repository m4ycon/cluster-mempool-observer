#![cfg_attr(feature = "strict", deny(warnings))]

use observer::{clients, infra::config::init_config, runner};
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

    if let Err(e) = nats::init(&config.nats).await {
        tracing::error!("Failed to connect to NATS server: {e:?}");
        std::process::exit(1);
    }

    if let Err(e) = clients::rpc_client::init(&config.rpc) {
        tracing::error!("Failed to initialize RPC client: {e:?}");
        std::process::exit(1);
    }

    tracing::info!("Starting mempool observer...");

    runner::run(&config).await;
}
