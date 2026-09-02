#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, NewMempoolDelta, NewTransaction, SystemEventKind};
use api::db::{
    BlockRepository, ClusterMembershipRepository, ClusterRepository, DbPool,
    MempoolAdmissionRepository, MempoolDeltaRepository, Repos, SnapshotRepository,
    SystemEventRepository, TransactionRepository,
};
use api::infra::deps::Deps;
use diesel_async::RunQueryDsl;
use shared::events::MempoolDeltaEvent;
use testkit::deps::{deps, inert_clients, isolated_deps};
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

    for stage in ["insert_new_txs", "sync_clusters"] {
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

// region: persist_failed system event

#[tokio::test]
async fn persist_adds_failure_records_a_persist_failed_system_event() {
    let read_only_pool = isolated_pool().await;
    make_pool_read_only(&read_only_pool).await;
    let writable_pool = isolated_pool().await;

    let repos = Repos {
        block: BlockRepository::new(writable_pool.clone()),
        cluster: ClusterRepository::new(writable_pool.clone()),
        cluster_membership: ClusterMembershipRepository::new(writable_pool.clone()),
        mempool_admission: MempoolAdmissionRepository::new(read_only_pool.clone()),
        mempool_delta: MempoolDeltaRepository::new(writable_pool.clone()),
        snapshot: SnapshotRepository::new(writable_pool.clone()),
        system_event: SystemEventRepository::new(writable_pool),
        transaction: TransactionRepository::new(read_only_pool),
    };
    let deps = Deps::new(repos, &inert_clients())
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::strict(vec![]));

    let service = deps.mempool_service();
    service
        .apply_delta(MempoolDeltaEvent {
            added: vec!["a".into()],
            removed: vec![],
        })
        .await;

    let events = deps
        .repos
        .system_event
        .list(None, None)
        .await
        .expect("list system events");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, SystemEventKind::PersistFailed);
    assert_eq!(events[0].details["stage"], "persist_adds");
    assert_eq!(events[0].details["added_count"], 1);
}

// endregion
