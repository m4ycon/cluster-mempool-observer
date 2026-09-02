use api::db::DbPool;
use api::db::build_pool;
use api::db::pool::build_pool_with_max_size;
use diesel_async::AsyncConnection;

/// Address of the database this test process owns. The `scripts/test-pg.sh`
/// nextest setup script prepares one per slot and publishes the base URL; slots
/// are never shared by two tests running at the same time, so fixtures with
/// fixed primary keys cannot collide across tests.
fn slot_url() -> String {
    let base = std::env::var("TEST_PG_URL_BASE").expect(
        "TEST_PG_URL_BASE is unset: database tests run under `cargo nextest run`, \
         which prepares Postgres through the `postgres` setup script",
    );
    let slot = std::env::var("NEXTEST_TEST_GLOBAL_SLOT").unwrap_or_else(|_| "0".to_string());
    format!("{base}/mempool_test_{slot}")
}

/// A size-1 pool against this process's slot database with a test transaction
/// already open on its single connection. Repositories built from this pool
/// reuse that connection, so every write rolls back when the pool is dropped -
/// giving each test isolation without teardown.
pub async fn isolated_pool() -> DbPool {
    let pool = build_pool_with_max_size(&slot_url(), 1).expect("build size-1 test pool");
    let mut conn = pool.get().await.expect("checkout test connection");
    conn.begin_test_transaction()
        .await
        .expect("begin test transaction");
    drop(conn);
    pool
}

/// A size-1 pool against this process's slot database with no test
/// transaction: statements commit for real, exactly like production.
///
/// `isolated_pool` cannot stand in for this: everything it does lives inside
/// one already-open, never-committed transaction, so a chunk that "commits"
/// under the bug is just as invisible afterward as one that was properly
/// rolled back -- there is nothing left to tell the two apart. Only for a
/// test that needs to observe commit behavior itself; the caller must clean
/// up whatever rows it leaves behind.
pub async fn autocommit_pool() -> DbPool {
    build_pool_with_max_size(&slot_url(), 1).expect("build size-1 test pool")
}

/// A pool pointing at a database that will never answer. Deadpool builds it
/// lazily, so it is free to construct and every query fails fast -- which is
/// exactly what instrumentation tests want. Needs no server.
pub fn inert_pool() -> DbPool {
    build_pool("postgres://user:pass@127.0.0.1:1/nothing").expect("pool builds lazily")
}
