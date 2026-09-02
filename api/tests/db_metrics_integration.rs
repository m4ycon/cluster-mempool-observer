#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, NewMempoolDelta, NewTransaction};
use api::db::{ClusterRepository, DbPool};
use diesel_async::RunQueryDsl;
use shared::events::MempoolDeltaEvent;
use testkit::deps::{deps, isolated_deps};
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

/// The two pipeline stages past the first successful query: reaching them
/// requires `existing_txids` to actually return.
#[tokio::test]
async fn delta_pipeline_records_its_stages_and_new_txs() {
    let deps = isolated_deps()
        .await
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default());
    let service = deps.mempool_service();

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    service
        .apply_delta(MempoolDeltaEvent {
            added: vec!["a".into(), "b".into()],
            removed: vec![],
        })
        .await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    for stage in ["persist_adds", "sync_clusters"] {
        assert_series(
            &rendered,
            &format!(r#"mempool_delta_stage_seconds_count{{stage="{stage}"}} 1"#),
        );
    }
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
    deps.repos
        .mempool_delta
        .insert_many(&[NewMempoolDelta {
            txid: "a".into(),
            reason: DeltaReason::AddMempool,
        }])
        .await
        .expect("seed delta");

    let service = deps.mempool_service();

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    service
        .apply_delta(MempoolDeltaEvent {
            added: vec!["a".into()],
            removed: vec![],
        })
        .await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(&rendered, "mempool_new_txs_total 0");
}

// region: mempool_persist_failed_total

/// Puts the pool's one connection into a read-only transaction: reads (like
/// `existing_txids`) keep succeeding, but every write the pipeline attempts
/// fails.
async fn make_pool_read_only(pool: &DbPool) {
    let mut conn = pool.get().await.expect("checkout connection");
    diesel::sql_query("SET TRANSACTION READ ONLY")
        .execute(&mut conn)
        .await
        .expect("set transaction read only");
}

#[tokio::test]
async fn persist_failed_total_counts_persist_adds_failures() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone())
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::strict(vec![]));

    make_pool_read_only(&pool).await;

    let service = deps.mempool_service();
    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    service
        .apply_delta(MempoolDeltaEvent {
            added: vec!["a".into()],
            removed: vec![],
        })
        .await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(
        &rendered,
        r#"mempool_persist_failed_total{stage="persist_adds"} 1"#,
    );
    assert_no_series(
        &rendered,
        r#"mempool_persist_failed_total{stage="existing_txids"}"#,
    );
    assert_no_series(
        &rendered,
        r#"mempool_persist_failed_total{stage="record_removes"}"#,
    );
}

/// `record_removes_for_unpaired` returns before writing anything when no
/// candidate has an unpaired `add_mempool`. Seeding one for "b" is what carries
/// the removal as far as its insert, which is the write the read-only pool has
/// to reject for this failure to happen at all.
#[tokio::test]
async fn persist_failed_total_counts_record_removes_failures() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone())
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::strict(vec![]));

    deps.repos
        .mempool_delta
        .insert_many(&[NewMempoolDelta {
            txid: "b".into(),
            reason: DeltaReason::AddMempool,
        }])
        .await
        .expect("seed unpaired add");

    make_pool_read_only(&pool).await;

    let service = deps.mempool_service();
    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    service
        .apply_delta(MempoolDeltaEvent {
            added: vec![],
            removed: vec!["b".into()],
        })
        .await;
    drop(guard);

    handle.run_upkeep();
    let rendered = handle.render();

    assert_series(
        &rendered,
        r#"mempool_persist_failed_total{stage="record_removes"} 1"#,
    );
    assert_no_series(
        &rendered,
        r#"mempool_persist_failed_total{stage="existing_txids"}"#,
    );
    assert_no_series(
        &rendered,
        r#"mempool_persist_failed_total{stage="persist_adds"}"#,
    );
}

// endregion
