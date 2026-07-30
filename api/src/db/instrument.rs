use crate::db::pool::DbPool;
use crate::db::repositories::RepoResult;
use diesel_async::AsyncPgConnection;
use shared::metrics::record_elapsed;
use std::time::Instant;

/// Time spent waiting for a pooled connection.
const DB_POOL_ACQUIRE_SECONDS: &str = "db_pool_acquire_seconds";

/// Query execution time, excluding the pool wait above.
const DB_QUERY_SECONDS: &str = "db_query_seconds";

/// Failed queries, counted rather than labelled onto the histogram -- errors
/// are rare and their latency is not interesting, but their rate is.
const DB_QUERY_ERRORS_TOTAL: &str = "db_query_errors_total";

/// Checkouts that never yielded a connection: pool exhausted, or the database
/// unreachable. The clearest backpressure signal there is, so it gets its own
/// counter rather than being folded into query errors.
const DB_POOL_ACQUIRE_ERRORS_TOTAL: &str = "db_pool_acquire_errors_total";

/// Pool occupancy, by `state`: `size`, `available`, `waiting`.
const DB_POOL_CONNECTIONS: &str = "db_pool_connections";

/// Runs `f` on a pooled connection, timing the pool wait and the query
/// separately.
///
/// The split matters: a slow repo call means either the database is slow or
/// every connection is busy, and those need different fixes. One combined
/// number cannot tell them apart.
///
/// `repo` and `op` must be literals, never values -- they are metric labels,
/// and a txid or cluster id there would blow up cardinality.
pub async fn query<T, F>(pool: &DbPool, repo: &'static str, op: &'static str, f: F) -> RepoResult<T>
where
    F: AsyncFnOnce(&mut AsyncPgConnection) -> Result<T, diesel::result::Error>,
{
    let waiting_since = Instant::now();
    let conn = pool.get().await;
    record_elapsed(DB_POOL_ACQUIRE_SECONDS, &[], waiting_since);
    let mut conn = match conn {
        Ok(conn) => conn,
        Err(e) => {
            metrics::counter!(DB_POOL_ACQUIRE_ERRORS_TOTAL, "repo" => repo, "op" => op)
                .increment(1);
            return Err(e.into());
        }
    };

    let running_since = Instant::now();
    let result = f(&mut conn).await;
    let labels = [("repo", repo), ("op", op)];
    record_elapsed(DB_QUERY_SECONDS, &labels, running_since);

    if result.is_err() {
        metrics::counter!(DB_QUERY_ERRORS_TOTAL, "repo" => repo, "op" => op).increment(1);
    }

    Ok(result?)
}

pub fn sample_pool(pool: &DbPool) {
    let status = pool.status();
    metrics::gauge!(DB_POOL_CONNECTIONS, "state" => "size").set(status.size as f64);
    metrics::gauge!(DB_POOL_CONNECTIONS, "state" => "available").set(status.available as f64);
    metrics::gauge!(DB_POOL_CONNECTIONS, "state" => "waiting").set(status.waiting as f64);
}
