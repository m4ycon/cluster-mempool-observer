#![cfg(feature = "db_integration_tests")]

//! Regression coverage for `NewTransaction::sorted_by_txid`, called from every
//! inserter into `transactions`. Each test races the function under test
//! against a second, manually-driven connection that always writes ascending,
//! surrounding a held txid with new txids on both sides -- the shape that
//! deadlocked `BlockService::apply_block` in production.
//!
//! See issue #15.

use api::db::models::{Cluster, NewCluster, NewTransaction};
use api::db::schema::{cluster_deltas, clusters, mempool_deltas, transactions};
use api::db::{
    ClusterMembershipRepository, DbPool, MempoolLedgerRepository, TransactionRepository,
};
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Nullable, Text};
use diesel_async::RunQueryDsl;
use shared::models::DeltaDirection;
use shared::snapshot::JournalEntry;
use std::collections::HashSet;
use std::future::Future;
use std::time::{Duration, Instant};
use testkit::fixtures::fixed_time;
use testkit::postgres::autocommit_pool_with_max_size;
use time::OffsetDateTime;

#[derive(QueryableByName)]
struct LockWaitCount {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

#[derive(QueryableByName)]
struct Activity {
    #[diesel(sql_type = Nullable<Text>)]
    backends: Option<String>,
}

fn padded(prefix: &str, n: u32) -> String {
    format!("{prefix}{n:04}")
}

fn id_range(prefix: &str, start: u32, end_inclusive: u32) -> Vec<String> {
    (start..=end_inclusive).map(|n| padded(prefix, n)).collect()
}

/// Plays "the other inserter" production raced and lost against: BEGIN,
/// insert `held_txid`, wait for `x` to block on it, then
/// insert everything above it ascending and commit. Sorted, `x` empties below
/// `held` and blocks there before touching anything above it, so this insert
/// never contends and the race resolves cleanly. Unsorted, `x` may already
/// hold some of those above-rows uncommitted, this insert blocks right back on
/// `x`, and Postgres reports a deadlock on one side or the other.
async fn assert_survives_racing_insert<T, Fut>(
    pool: DbPool,
    held_txid: &str,
    above_held: &[String],
    x: Fut,
) -> T
where
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let mut t0 = pool.get().await.expect("checkout T0 connection");
    // Opens x's connection before the race. Establishing it while T0 holds its
    // transaction and polls could stall in the handshake, which is not what
    // these tests are about.
    drop(pool.get().await.expect("pre-warm x's connection"));
    diesel::sql_query("BEGIN")
        .execute(&mut t0)
        .await
        .expect("T0 begin");
    diesel::insert_into(transactions::table)
        .values(&NewTransaction::hollow(held_txid))
        .execute(&mut t0)
        .await
        .expect("T0 insert the held row");

    let x_handle = tokio::spawn(x);

    // Generous on purpose: the loop exits as soon as `x` blocks, so the ceiling
    // only bounds a stall, never a pass.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let blocked_by_t0: LockWaitCount = diesel::sql_query(
            "SELECT count(*) AS count FROM pg_stat_activity \
             WHERE pg_backend_pid() = ANY(pg_blocking_pids(pid))",
        )
        .get_result(&mut t0)
        .await
        .expect("poll for a backend blocked by T0");
        if blocked_by_t0.count > 0 {
            break;
        }
        if x_handle.is_finished() {
            match x_handle.await {
                Ok(_) => panic!("the racing insert finished without blocking on the held row"),
                Err(e) => panic!("the racing insert failed before blocking on the held row: {e}"),
            }
        }
        if Instant::now() >= deadline {
            let activity: Activity = diesel::sql_query(
                "SELECT string_agg(format('%s %s %s/%s %s', pid, state, wait_event_type, \
                 wait_event, left(query, 80)), E'\\n') AS backends \
                 FROM pg_stat_activity \
                 WHERE datname = current_database() AND pid <> pg_backend_pid()",
            )
            .get_result(&mut t0)
            .await
            .expect("read pg_stat_activity");
            panic!(
                "timed out waiting for the racing insert to block on the held row; \
                 other backends:\n{}",
                activity.backends.unwrap_or_default()
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut above_sorted = above_held.to_vec();
    above_sorted.sort();
    let above_rows: Vec<NewTransaction> = above_sorted
        .iter()
        .map(|t| NewTransaction::hollow(t))
        .collect();
    let above_refs: Vec<&NewTransaction> = above_rows.iter().collect();
    diesel::insert_into(transactions::table)
        .values(above_refs)
        .execute(&mut t0)
        .await
        .expect(
            "T0 blocked inserting the txids above the held row: the inserter under test no \
             longer writes ascending by txid",
        );
    diesel::sql_query("COMMIT")
        .execute(&mut t0)
        .await
        .expect("T0 commit");

    x_handle.await.expect(
        "the racing insert deadlocked against T0: the inserter under test no longer writes \
         ascending by txid",
    )
}

#[tokio::test]
async fn insert_or_confirm_many_survives_a_racing_hollow_insert() {
    let pool = autocommit_pool_with_max_size(3).await;
    let prefix = "deadlock-block-";
    let held = padded(prefix, 500);
    let below = [padded(prefix, 100), padded(prefix, 200)];
    let above = [padded(prefix, 800), padded(prefix, 900)];

    // deliberately non-ascending: above-rows first, then held, then below-rows
    let order: Vec<String> = above
        .iter()
        .cloned()
        .chain(std::iter::once(held.clone()))
        .chain(below.iter().cloned())
        .collect();
    let txs: Vec<NewTransaction> = order.iter().map(|t| NewTransaction::hollow(t)).collect();

    let repo = TransactionRepository::new(pool.clone());
    let x = async move {
        repo.insert_or_confirm_many(&txs).await.expect(
            "insert_or_confirm_many must sort ascending by txid, or a concurrent hollow \
             insert of the same new txids deadlocks",
        )
    };
    assert_survives_racing_insert(pool.clone(), &held, &above, x).await;

    let mut conn = pool.get().await.expect("cleanup conn");
    let count: i64 = transactions::table
        .filter(transactions::txid.like(format!("{prefix}%")))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count rows");
    diesel::delete(transactions::table.filter(transactions::txid.like(format!("{prefix}%"))))
        .execute(&mut conn)
        .await
        .expect("cleanup transactions rows");

    assert_eq!(
        count, 5,
        "every txid on both sides of the race must have landed"
    );
}

#[tokio::test]
async fn insert_many_survives_a_racing_hollow_insert() {
    let pool = autocommit_pool_with_max_size(3).await;
    let prefix = "deadlock-bootstrap-";
    let held = padded(prefix, 500);
    let below = [padded(prefix, 100), padded(prefix, 200)];
    let above = [padded(prefix, 800), padded(prefix, 900)];

    let order: Vec<String> = above
        .iter()
        .cloned()
        .chain(std::iter::once(held.clone()))
        .chain(below.iter().cloned())
        .collect();
    let txs: Vec<NewTransaction> = order.iter().map(|t| NewTransaction::hollow(t)).collect();

    let repo = TransactionRepository::new(pool.clone());
    let x = async move {
        repo.insert_many(&txs).await.expect(
            "insert_many must sort ascending by txid, or a concurrent hollow insert of the \
             same new txids deadlocks",
        )
    };
    assert_survives_racing_insert(pool.clone(), &held, &above, x).await;

    let mut conn = pool.get().await.expect("cleanup conn");
    let count: i64 = transactions::table
        .filter(transactions::txid.like(format!("{prefix}%")))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count rows");
    diesel::delete(transactions::table.filter(transactions::txid.like(format!("{prefix}%"))))
        .execute(&mut conn)
        .await
        .expect("cleanup transactions rows");

    assert_eq!(
        count, 5,
        "every txid on both sides of the race must have landed"
    );
}

#[tokio::test]
async fn write_batch_survives_a_racing_hollow_insert() {
    let pool = autocommit_pool_with_max_size(3).await;
    let prefix = "deadlock-ledger-";
    let held = padded(prefix, 500);
    let below = id_range(prefix, 1, 24);
    let above = id_range(prefix, 600, 624);

    // order fed to write_batch does not matter: distinct_txids collects it
    // into a HashSet, so the insertion order it produces is unsorted-random
    let all: Vec<String> = below
        .iter()
        .cloned()
        .chain(std::iter::once(held.clone()))
        .chain(above.iter().cloned())
        .collect();
    let entries: Vec<JournalEntry> = all
        .iter()
        .map(|t| JournalEntry {
            txid: t.clone(),
            direction: DeltaDirection::Add,
            observed_at: OffsetDateTime::now_utc(),
        })
        .collect();

    let repo = MempoolLedgerRepository::new(pool.clone());
    let x = async move {
        repo.write_batch(&entries, &|| false).await.expect(
            "write_batch's hollow insert must sort ascending by txid, or a concurrent hollow \
             insert of the same new txids deadlocks",
        )
    };
    assert_survives_racing_insert(pool.clone(), &held, &above, x).await;

    let mut conn = pool.get().await.expect("cleanup conn");
    let count: i64 = transactions::table
        .filter(transactions::txid.like(format!("{prefix}%")))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count rows");
    diesel::delete(mempool_deltas::table.filter(mempool_deltas::txid.like(format!("{prefix}%"))))
        .execute(&mut conn)
        .await
        .expect("cleanup mempool_deltas rows");
    diesel::delete(transactions::table.filter(transactions::txid.like(format!("{prefix}%"))))
        .execute(&mut conn)
        .await
        .expect("cleanup transactions rows");

    assert_eq!(
        count, 50,
        "every txid on both sides of the race must have landed"
    );
}

#[tokio::test]
async fn insert_with_members_survives_a_racing_hollow_insert() {
    let pool = autocommit_pool_with_max_size(3).await;
    let prefix = "deadlock-cluster-";
    let held = padded(prefix, 500);
    let below = id_range(prefix, 1, 24);
    let above = id_range(prefix, 600, 624);

    // NewCluster.txids order does not matter: this is what production hands
    // insert_hollow_members after building a member set through a HashSet
    let members: HashSet<String> = below
        .iter()
        .cloned()
        .chain(std::iter::once(held.clone()))
        .chain(above.iter().cloned())
        .collect();
    let new_cluster = NewCluster {
        txids: members.into_iter().collect(),
        total_vsize: 1,
        total_fee: 1,
        first_seen_at: fixed_time(),
    };

    let repo = ClusterMembershipRepository::new(pool.clone());
    let x = async move {
        repo.insert_with_members(&new_cluster).await.expect(
            "insert_hollow_members must sort ascending by txid, or a concurrent hollow \
             insert of the same new txids deadlocks",
        )
    };
    let cluster: Cluster = assert_survives_racing_insert(pool.clone(), &held, &above, x).await;
    assert_eq!(cluster.txids.len(), 50);

    let mut conn = pool.get().await.expect("cleanup conn");
    let count: i64 = transactions::table
        .filter(transactions::txid.like(format!("{prefix}%")))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count rows");
    diesel::delete(cluster_deltas::table.filter(cluster_deltas::cluster_id.eq(cluster.id)))
        .execute(&mut conn)
        .await
        .expect("cleanup cluster_deltas row");
    diesel::delete(clusters::table.filter(clusters::id.eq(cluster.id)))
        .execute(&mut conn)
        .await
        .expect("cleanup clusters row");
    diesel::delete(transactions::table.filter(transactions::txid.like(format!("{prefix}%"))))
        .execute(&mut conn)
        .await
        .expect("cleanup transactions rows");

    assert_eq!(
        count, 50,
        "every txid on both sides of the race must have landed"
    );
}
