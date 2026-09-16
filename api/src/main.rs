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
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, oneshot};

/// How often startup re-checks a node that is down or still syncing.
const NODE_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// How long shutdown waits for the tx backfill consumer to drain.
const TX_INPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// How long shutdown waits for the mempool reconciler to drain. The tick
/// interval is 1s, so a few seconds is generous.
const MEMPOOL_RECONCILER_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

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
    let deps = Deps::with_queue_capacity(
        Repos::new(db_pool.clone()),
        &clients,
        cfg.tx_backfill_queue_capacity,
    );
    let state = deps.app_state();
    api::infra::metrics::spawn_samplers(db_pool.clone(), clients.pubsub.clone());
    api::infra::metrics::spawn_storage_sampler(db_pool, api::infra::disk::root(&cfg.data_dir));

    let app = api::infra::router::build(state.clone());

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .expect("failed to bind listener");

    tracing::info!("api listening on {}", cfg.bind);
    lifecycle::record_server_started(&state.system_event_service, &cfg).await;

    let feerate_diagram_snapshot = deps.feerate_diagram_snapshot.clone();
    let bootstrap_service = state.bootstrap_service.clone();
    let bootstrap_cfg = cfg.clone();
    let chain_retriever = ChainRpcRetriever::new(clients.rpc.clone());
    let node_health_service = deps.node_health_service.clone();
    let readiness = state.readiness.clone();

    let tx_backfill_shutdown = Arc::new(Notify::new());
    let consumer_shutdown = tx_backfill_shutdown.clone();
    let tx_backfill_rx = deps
        .tx_backfill_queue
        .take_receiver()
        .expect("tx backfill receiver not taken yet");
    let (consumer_handle_tx, consumer_handle_rx) = oneshot::channel();

    let mempool_reconciler_shutdown = Arc::new(Notify::new());
    let reconciler_shutdown = mempool_reconciler_shutdown.clone();
    let (reconciler_handle_tx, reconciler_handle_rx) = oneshot::channel();

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
            .run(&bootstrap_cfg, clients, feerate_diagram_snapshot)
            .await;
        readiness.set_phase(Phase::Ready);
        tracing::info!("bootstrap complete, serving data routes");

        let gauge_sample_service = deps.gauge_sample_service();
        let gauge_interval = Duration::from_secs(cfg.gauge_interval_secs);
        tokio::spawn(async move { gauge_sample_service.run(gauge_interval).await });

        let counter_sample_service = deps.counter_sample_service();
        tokio::spawn(async move { counter_sample_service.run(gauge_interval).await });

        let tx_backfill_consumer = deps.tx_backfill_consumer();
        let consumer_task = tokio::spawn(async move {
            tx_backfill_consumer
                .consume(tx_backfill_rx, consumer_shutdown)
                .await;
        });
        let _ = consumer_handle_tx.send(consumer_task);

        let mempool_reconciler = deps.mempool_reconciler();
        let reconciler_task = tokio::spawn(async move {
            mempool_reconciler.run(reconciler_shutdown).await;
        });
        let _ = reconciler_handle_tx.send(reconciler_task);
    });

    let shutdown = async move {
        lifecycle::record_server_stopped(
            &state.system_event_service,
            start,
            lifecycle::wait_for_shutdown_signal(),
        )
        .await;

        lifecycle::drain_tx_backfill_consumer(
            &tx_backfill_shutdown,
            consumer_handle_rx,
            TX_INPUT_DRAIN_TIMEOUT,
        )
        .await;

        lifecycle::drain_mempool_reconciler(
            &mempool_reconciler_shutdown,
            reconciler_handle_rx,
            MEMPOOL_RECONCILER_DRAIN_TIMEOUT,
        )
        .await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .expect("server error");
}
