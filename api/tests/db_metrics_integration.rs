#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, NewMempoolDelta, NewTransaction};
use api::db::{
    ClusterMembershipRepository, ClusterRepository, DbPool, MempoolDeltaRepository,
    TransactionRepository,
};
use api::services::cluster::ClusterService;
use api::services::cluster_delta::ClusterDeltaService;
use api::services::mempool::MempoolService;
use api::services::pubsub::PubSubService;
use shared::events::MempoolDeltaEvent;
use shared::pubsub::PubSub;
use shared::snapshot::ClusterSnapshot;
use testkit::metrics::{assert_no_series, assert_series, local_recorder};
use testkit::mocks::{MockClusterRetriever, MockTransactionRetriever};
use testkit::postgres::isolated_pool;

fn cluster_service(pool: DbPool) -> ClusterService<MockClusterRetriever> {
    ClusterService::new(
        ClusterRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        ClusterMembershipRepository::new(pool),
        MockClusterRetriever::default(),
        ClusterDeltaService::new(
            ClusterSnapshot::default(),
            PubSubService::new(PubSub::new()),
        ),
    )
}

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
    let pool = isolated_pool().await;
    let service = MempoolService::new(
        MempoolDeltaRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        MockTransactionRetriever::default(),
        cluster_service(pool),
        PubSubService::new(PubSub::new()),
    );

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

    for stage in ["fetch_new_txs", "sync_clusters"] {
        assert_series(
            &rendered,
            &format!(r#"mempool_delta_stage_seconds_count{{stage="{stage}"}} 1"#),
        );
    }
    // Neither txid was stored before, so both had to be fetched.
    assert_series(&rendered, "mempool_new_txs_total 2");
}

/// Already-stored txids are not re-fetched, so the counter must not move.
#[tokio::test]
async fn new_txs_total_excludes_already_stored_txids() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    tx_repo
        .insert(&NewTransaction::hollow("a"))
        .await
        .expect("seed transaction");
    MempoolDeltaRepository::new(pool.clone())
        .insert_many(&[NewMempoolDelta {
            txid: "a".into(),
            reason: DeltaReason::AddMempool,
        }])
        .await
        .expect("seed delta");

    let service = MempoolService::new(
        MempoolDeltaRepository::new(pool.clone()),
        tx_repo,
        MockTransactionRetriever::default(),
        cluster_service(pool),
        PubSubService::new(PubSub::new()),
    );

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
