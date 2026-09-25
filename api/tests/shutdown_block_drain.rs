use api::infra::deps::Deps;
use api::infra::lifecycle::stop_applying_blocks;
use shared::events::BlockConnectedEvent;
use std::sync::Arc;
use std::time::Duration;
use testkit::fixtures::BlockFixture;
use testkit::mocks::{BlockRetrieverPause, MockBlockRetriever, MockClusterRetriever};

const DRAIN_TIMEOUT: Duration = Duration::from_secs(280);

type MockDeps =
    Deps<observer::retrievers::TransactionRpcRetriever, MockClusterRetriever, MockBlockRetriever>;

fn with_paused_block(deps: Deps) -> (MockDeps, Arc<BlockRetrieverPause>) {
    let block = BlockFixture::new("blk1", 1)
        .with_txs(&[("tx1", 100)])
        .build();
    let (retriever, pause) = MockBlockRetriever::with_blocks(vec![block]).pausing();
    let deps = deps
        .with_cluster_retriever(MockClusterRetriever::default())
        .with_block_retriever(retriever);
    (deps, pause)
}

fn spawn_apply(deps: &MockDeps) -> tokio::task::JoinHandle<()> {
    let block_service = deps.block_service();
    tokio::spawn(async move {
        let _ = block_service
            .apply_block(BlockConnectedEvent {
                hash: "blk1".to_string(),
            })
            .await;
    })
}

#[cfg(feature = "db_integration_tests")]
#[tokio::test]
async fn shutdown_returns_only_once_the_in_flight_block_has_landed() {
    use api::db::schema::blocks;
    use diesel::prelude::*;
    use diesel_async::RunQueryDsl;

    let pool = testkit::postgres::isolated_pool().await;
    let (deps, pause) = with_paused_block(testkit::deps::deps(pool.clone()));

    let applying = spawn_apply(&deps);
    pause.wait_entered().await;

    // A separate instance, as main's is not the one bootstrap applies through.
    let shutdown_handle = deps.block_service();
    let mut stopping =
        tokio::spawn(async move { stop_applying_blocks(&shutdown_handle, DRAIN_TIMEOUT).await });

    tokio::time::pause();
    assert!(
        tokio::time::timeout(Duration::from_secs(60), &mut stopping)
            .await
            .is_err(),
        "the block is still in flight, so shutdown must keep waiting"
    );
    tokio::time::resume();

    pause.release();
    stopping.await.expect("stop_applying_blocks panicked");

    let mut conn = pool.get().await.expect("conn");
    let heights: Vec<i64> = blocks::table
        .select(blocks::height)
        .load(&mut conn)
        .await
        .expect("load heights");
    assert_eq!(
        heights,
        vec![1],
        "shutdown returned before the block landed"
    );
    applying.await.expect("apply_block panicked");
}

#[tokio::test(start_paused = true)]
async fn no_block_starts_once_shutdown_has_stopped_applying() {
    let (deps, pause) = with_paused_block(testkit::deps::inert_deps());

    stop_applying_blocks(&deps.block_service(), DRAIN_TIMEOUT).await;

    let _applying = spawn_apply(&deps);
    assert!(
        tokio::time::timeout(Duration::from_secs(3600), pause.wait_entered())
            .await
            .is_err(),
        "a block started after shutdown stopped applying"
    );
}

#[tokio::test(start_paused = true)]
async fn shutdown_gives_up_on_a_block_that_outlasts_the_timeout() {
    let (deps, pause) = with_paused_block(testkit::deps::inert_deps());

    let applying = spawn_apply(&deps);
    pause.wait_entered().await;

    let started = tokio::time::Instant::now();
    stop_applying_blocks(&deps.block_service(), DRAIN_TIMEOUT).await;

    assert!(started.elapsed() >= DRAIN_TIMEOUT);
    assert!(!applying.is_finished(), "the block never left retrieval");
}
