use api::db::DbPool;
use api::db::pool::build_pool_with_max_size;
use diesel_async::AsyncConnection;
use std::time::Duration;
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

/// A running ephemeral Postgres container with migrations already applied.
/// Keep the `container` alive for as long as the DB is needed.
pub struct PgFixture {
    pub container: ContainerAsync<Postgres>,
    pub url: String,
}

/// Spin up a fresh Postgres container and apply migrations.
pub async fn setup_postgres() -> PgFixture {
    let container = Postgres::default()
        .start()
        .await
        .expect("start postgres container");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("get postgres host port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    run_migrations_with_retry(&url).await;

    PgFixture { container, url }
}

/// Run migrations with a retry loop, since the container may not be ready yet. (intermittent failures)
async fn run_migrations_with_retry(url: &str) {
    const ATTEMPTS: usize = 20;
    for attempt in 1..=ATTEMPTS {
        match api::db::run_migrations(url) {
            Ok(()) => return,
            Err(e) if attempt == ATTEMPTS => {
                panic!("apply migrations failed after {ATTEMPTS} attempts: {e}")
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
}

static SHARED: OnceCell<PgFixture> = OnceCell::const_new();

/// A process-wide Postgres container, booted and migrated once per test binary.
pub async fn shared_postgres() -> &'static str {
    SHARED.get_or_init(setup_postgres).await.url.as_str()
}

/// A size-1 pool against the shared container with a test transaction already
/// open on its single connection. Repositories built from this pool reuse that
/// connection, so every write rolls back when the pool is dropped - giving each
/// test isolation without teardown. Each test gets its own pool/connection, so
/// parallel tests stay isolated from one another.
pub async fn isolated_pool() -> DbPool {
    let url = shared_postgres().await;
    let pool = build_pool_with_max_size(url, 1).expect("build size-1 test pool");
    let mut conn = pool.get().await.expect("checkout test connection");
    conn.begin_test_transaction()
        .await
        .expect("begin test transaction");
    drop(conn);
    pool
}
