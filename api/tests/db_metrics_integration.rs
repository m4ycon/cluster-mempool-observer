#![cfg(feature = "db_integration_tests")]

use api::db::models::NewTransaction;
use api::db::{ClusterRepository, DbPool};
use diesel_async::RunQueryDsl;
use std::collections::HashMap;
use testkit::deps::{cluster_service, deps, isolated_deps};
use testkit::fixtures::{ClusterFixture, fixed_time};
use testkit::metrics::{assert_no_series, assert_series, local_recorder};
use testkit::mocks::{MockClusterRetriever, MockTransactionRetriever};
use testkit::postgres::isolated_pool;

#[tokio::test]
async fn successful_query_records_its_duration_and_no_error() {
    let pool = isolated_pool().await;
    let repo = ClusterRepository::new(pool);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    repo.count().await.expect("count succeeds");
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(
        &rendered,
        r#"db_query_seconds_count{repo="cluster",op="count"} 1"#,
    );
    assert_series(&rendered, "db_pool_acquire_seconds_count 1");
    assert_no_series(&rendered, "db_query_errors_total");
    assert_no_series(&rendered, "db_pool_acquire_errors_total");
}

/// A query that reaches the database and is rejected must still be timed --
/// its duration is real -- and counted separately from a checkout failure.
#[tokio::test]
async fn failing_query_is_timed_and_counted() {
    let pool = isolated_pool().await;
    let repo = ClusterRepository::new(pool);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    // No such cluster: `get_result` yields `NotFound` rather than a pool error.
    repo.update(i64::MAX, &["a".into()], 1, 1)
        .await
        .expect_err("update of a missing cluster fails");
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(
        &rendered,
        r#"db_query_seconds_count{repo="cluster",op="update"} 1"#,
    );
    assert_series(
        &rendered,
        r#"db_query_errors_total{repo="cluster",op="update"} 1"#,
    );
    assert_no_series(&rendered, "db_pool_acquire_errors_total");
}

/// A tick's write lands two new txids hollow, so both count toward
/// `mempool_new_txs_total`.
#[tokio::test]
async fn tick_records_new_txs_total() {
    let deps = isolated_deps()
        .await
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default());
    let reconciler = deps.mempool_reconciler();
    deps.mempool_ledger
        .assert_present(&["a".to_string(), "b".to_string()]);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    reconciler.tick().await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    // Neither txid was stored before, so both were inserted hollow.
    assert_series(&rendered, "mempool_new_txs_total 2");
}

/// Already-stored txids are not re-fetched, so the counter must not move.
#[tokio::test]
async fn new_txs_total_excludes_already_stored_txids() {
    let deps = isolated_deps()
        .await
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default());
    deps.repos
        .transaction
        .insert(&NewTransaction::hollow("a"))
        .await
        .expect("seed transaction");

    let reconciler = deps.mempool_reconciler();
    deps.mempool_ledger.assert_present(&["a".to_string()]);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    reconciler.tick().await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(&rendered, "mempool_new_txs_total 0");
}

// region: mempool_delta_txs_total

#[tokio::test]
async fn tick_counts_added_txids() {
    let deps = isolated_deps()
        .await
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default());
    let reconciler = deps.mempool_reconciler();
    deps.mempool_ledger
        .assert_present(&["a".to_string(), "b".to_string(), "c".to_string()]);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    reconciler.tick().await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(&rendered, r#"mempool_delta_txs_total{direction="added"} 3"#);
}

#[tokio::test]
async fn tick_counts_removed_txids() {
    let deps = isolated_deps()
        .await
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default());
    let reconciler = deps.mempool_reconciler();
    deps.mempool_ledger
        .assert_present(&["a".to_string(), "b".to_string()]);
    reconciler.tick().await; // commit the adds so the removes below are real transitions

    deps.mempool_ledger
        .assert_absent(&["a".to_string(), "b".to_string()]);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    reconciler.tick().await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(
        &rendered,
        r#"mempool_delta_txs_total{direction="removed"} 2"#,
    );
}

#[tokio::test]
async fn tick_accumulates_txs_total_across_ticks() {
    let deps = isolated_deps()
        .await
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default());
    let reconciler = deps.mempool_reconciler();

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    deps.mempool_ledger.assert_present(&["a".to_string()]);
    reconciler.tick().await;
    deps.mempool_ledger
        .assert_present(&["b".to_string(), "c".to_string()]);
    reconciler.tick().await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(&rendered, r#"mempool_delta_txs_total{direction="added"} 3"#);
}

// endregion

// region: mempool_persist_failed_total

/// Puts the pool's one connection into a read-only transaction: reads keep
/// succeeding, but the reconciler's write transaction fails.
async fn make_pool_read_only(pool: &DbPool) {
    let mut conn = pool.get().await.expect("checkout connection");
    diesel::sql_query("SET TRANSACTION READ ONLY")
        .execute(&mut conn)
        .await
        .expect("set transaction read only");
}

#[tokio::test]
async fn persist_failed_total_counts_write_batch_failures() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone())
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::strict(vec![]));

    make_pool_read_only(&pool).await;

    let reconciler = deps.mempool_reconciler();
    deps.mempool_ledger.assert_present(&["a".to_string()]);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    reconciler.tick().await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(
        &rendered,
        r#"mempool_persist_failed_total{stage="write_batch"} 1"#,
    );
}

// endregion

// region: cluster_confirm_mined_stage_seconds

/// Renders the metrics of one `confirm_mined` over a block mining `mined`,
/// after a sync round has stored `clusters`.
async fn confirm_mined_metrics(clusters: &[&[&str]], mined: &[&str]) -> String {
    let fixtures = clusters
        .iter()
        .map(|txids| ClusterFixture::new(txids).build())
        .collect();
    let svc = cluster_service(isolated_pool().await, fixtures);
    let candidates: Vec<String> = clusters.iter().map(|txids| txids[0].to_string()).collect();
    svc.sync_clusters_for(&candidates, &[]).await;

    let mined: Vec<String> = mined.iter().map(|txid| txid.to_string()).collect();
    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    svc.confirm_mined(&mined, &HashMap::new(), &HashMap::new(), fixed_time())
        .await;
    drop(guard);

    handle.run_upkeep();
    handle.render()
}

#[tokio::test]
async fn confirm_mined_times_every_stage_once_per_block() {
    // {a, b} is mined whole, {c, d} only in part, and {e, f} not at all.
    let rendered =
        confirm_mined_metrics(&[&["a", "b"], &["c", "d"], &["e", "f"]], &["a", "b", "c"]).await;

    for stage in [
        "lookup",
        "confirm_full",
        "confirm_partial",
        "partial_resync",
    ] {
        assert_series(
            &rendered,
            &format!(r#"cluster_confirm_mined_stage_seconds_count{{stage="{stage}"}} 1"#),
        );
    }
    assert_series(
        &rendered,
        r#"cluster_confirm_mined_clusters_total{kind="full"} 1"#,
    );
    assert_series(
        &rendered,
        r#"cluster_confirm_mined_clusters_total{kind="partial"} 1"#,
    );
}

#[tokio::test]
async fn confirm_mined_counts_every_cluster_but_samples_each_stage_once() {
    let rendered = confirm_mined_metrics(&[&["a", "b"], &["c", "d"]], &["a", "b", "c", "d"]).await;

    assert_series(
        &rendered,
        r#"cluster_confirm_mined_stage_seconds_count{stage="confirm_full"} 1"#,
    );
    assert_series(
        &rendered,
        r#"cluster_confirm_mined_clusters_total{kind="full"} 2"#,
    );
    assert_series(
        &rendered,
        r#"cluster_confirm_mined_clusters_total{kind="partial"} 0"#,
    );
    for stage in ["confirm_partial", "partial_resync"] {
        assert_no_series(
            &rendered,
            &format!(r#"cluster_confirm_mined_stage_seconds_count{{stage="{stage}"}}"#),
        );
    }
}

// endregion

#[tokio::test]
async fn database_size_is_sampled_as_a_plausible_byte_count() {
    let pool = isolated_pool().await;

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    api::db::instrument::sample_database_size(&pool).await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    let value: f64 = rendered
        .lines()
        .find_map(|line| line.strip_prefix("database_size_bytes "))
        .unwrap_or_else(|| panic!("expected database_size_bytes in:\n{rendered}"))
        .parse()
        .expect("database size is a number");

    // A freshly migrated database is megabytes, never kilobytes or terabytes.
    assert!(
        (1_000_000.0..1e12).contains(&value),
        "implausible database size: {value} bytes"
    );
    assert_no_series(&rendered, "db_query_errors_total");
}
