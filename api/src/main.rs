#![cfg_attr(feature = "strict", deny(warnings))]

use api::db;
use api::db::Repos;
use api::infra::config::ApiConfig;
use api::infra::deps::Deps;
use observer::clients::Clients;

fn main() {
    let cfg = ApiConfig::from_env().expect("failed to load config");

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(run(cfg));
}

async fn run(cfg: ApiConfig) {
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

    let mempool_snapshot = deps.mempool_snapshot.clone();
    state
        .bootstrap_service
        .run(&cfg, clients, mempool_snapshot)
        .await;

    axum::serve(listener, app).await.expect("server error");
}
