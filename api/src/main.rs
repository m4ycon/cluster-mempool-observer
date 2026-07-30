#![cfg_attr(feature = "strict", deny(warnings))]

use api::db;
use api::infra::config::ApiConfig;
use api::infra::state::AppState;

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

    let (state, snapshot, clients) = AppState::build(&cfg.observer, db_pool.clone());
    api::infra::metrics::spawn_samplers(db_pool, clients.pubsub.clone());

    let app = api::infra::router::build(state.clone());

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .expect("failed to bind listener");

    tracing::info!("api listening on {}", cfg.bind);

    state.bootstrap_service.run(&cfg, clients, snapshot).await;

    axum::serve(listener, app).await.expect("server error");
}
