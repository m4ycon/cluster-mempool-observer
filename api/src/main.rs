#![cfg_attr(feature = "strict", deny(warnings))]

use api::db;
use api::db::Repos;
use api::infra::config::ApiConfig;
use api::infra::deps::Deps;
use api::infra::lifecycle;
use api::infra::node_wait;
use api::infra::readiness::Phase;
use observer::clients::Clients;
use observer::retrievers::ChainRpcRetriever;
use std::time::{Duration, Instant};

/// How often startup re-checks a node that is down or still syncing.
const NODE_POLL_INTERVAL: Duration = Duration::from_secs(10);

fn main() {
    let cfg = ApiConfig::from_env().expect("failed to load config");

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(run(cfg));
}

async fn run(cfg: ApiConfig) {
    let start = Instant::now();
    let _log_guard = shared::logging::init_tracing(&cfg.logging);
    api::infra::metrics::serve(&cfg.metrics).await;

    db::run_migrations(&cfg.database.url).expect("failed to run migrations");
    let db_pool = db::build_pool(&cfg.database.url).expect("failed to build db pool");

    let clients = Clients::new(&cfg.observer).expect("failed to initialize node clients");
    let deps = Deps::new(Repos::new(db_pool.clone()), &clients);
    let state = deps.app_state();
    api::infra::metrics::spawn_samplers(db_pool, clients.pubsub.clone());

    let app = api::infra::router::build(state.clone());

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .expect("failed to bind listener");

    tracing::info!("api listening on {}", cfg.bind);
    lifecycle::record_server_started(&state.system_event_service, &cfg).await;

    let mempool_snapshot = deps.mempool_snapshot.clone();
    let feerate_diagram_snapshot = deps.feerate_diagram_snapshot.clone();
    let bootstrap_service = state.bootstrap_service.clone();
    let bootstrap_cfg = cfg.clone();
    let chain_retriever = ChainRpcRetriever::new(clients.rpc.clone());
    let node_health_service = deps.node_health_service.clone();
    let readiness = state.readiness.clone();
    let snapshot_service = deps.snapshot_service();
    let snapshot_interval = Duration::from_secs(cfg.snapshot_interval_secs);
    tokio::spawn(async move {
        node_wait::wait_until_ready(
            &chain_retriever,
            &node_health_service,
            &readiness,
            NODE_POLL_INTERVAL,
        )
        .await;

        readiness.set_phase(Phase::Bootstrapping);
        bootstrap_service
            .run(
                &bootstrap_cfg,
                clients,
                mempool_snapshot,
                feerate_diagram_snapshot,
            )
            .await;
        readiness.set_phase(Phase::Ready);
        tracing::info!("bootstrap complete, serving data routes");

        tokio::spawn(async move { snapshot_service.run(snapshot_interval).await });
    });

    let shutdown = async move {
        lifecycle::record_server_stopped(
            &state.system_event_service,
            start,
            lifecycle::wait_for_shutdown_signal(),
        )
        .await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .expect("server error");
}
