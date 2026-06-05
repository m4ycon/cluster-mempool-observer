pub mod clients;
pub mod config;
pub mod extractors;
pub mod publisher;
pub mod runner;

use std::path::Path;

use crate::config::{CONFIG_PATH, Config};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = match Config::load(Path::new(CONFIG_PATH)) {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = clients::rpc_client::init(&config.rpc) {
        tracing::error!("Failed to initialize RPC client: {e:?}");
        std::process::exit(1);
    }

    tracing::info!("Starting mempool observer...");

    runner::run(config.poll_interval_secs).await;
}
