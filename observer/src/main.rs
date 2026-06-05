use observer::{
    clients,
    infra::{config::init_config, logging::init_tracing},
    runner,
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
