pub mod clients;
pub mod config;
pub mod extractors;
mod logging;
pub mod publisher;
pub mod runner;

use crate::{
    config::init_config,
    logging::init_tracing,
};

#[tokio::main]
async fn main() {
    init_tracing();

    let config = init_config();

    if let Err(e) = clients::rpc_client::init(&config.rpc) {
        tracing::error!("Failed to initialize RPC client: {e:?}");
        std::process::exit(1);
    }

    tracing::info!("Starting mempool observer...");

    runner::run(config.poll_interval_secs).await;
}
