#![cfg(feature = "db_integration_tests")]

//! Issue #7.
//!
//! `write_batch` picks the removal reason by asking whether
//! `transactions.confirmed_at` is set, but `apply_block` only fills that column
//! well into its run. A flush landing first labels the whole batch evicted.
//!
//! Asserted by outcome rather than by naming the gate, so they survive a change
//! of mechanism.

use api::db::DbPool;
use api::db::models::DeltaReason;
use api::db::schema::mempool_deltas;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::events::BlockConnectedEvent;
use testkit::deps::deps;
use testkit::fixtures::BlockFixture;
use testkit::metrics::{assert_series, capture};
use testkit::mocks::{MockBlockRetriever, MockClusterRetriever};
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

async fn delta_reasons_in_order(pool: &DbPool, txid: &str) -> Vec<DeltaReason> {
    let mut conn = pool.get().await.expect("checkout connection");
    mempool_deltas::table
        .filter(mempool_deltas::txid.eq(txid))
        .order(mempool_deltas::id.asc())
        .select(mempool_deltas::reason)
        .load(&mut conn)
        .await
        .expect("load deltas")
}

async fn delta_created_at(pool: &DbPool, txid: &str) -> OffsetDateTime {
    let mut conn = pool.get().await.expect("checkout connection");
    mempool_deltas::table
        .filter(mempool_deltas::txid.eq(txid))
        .order(mempool_deltas::id.asc())
        .select(mempool_deltas::created_at)
        .first(&mut conn)
        .await
        .expect("load created_at")
}

#[tokio::test]
async fn a_flush_racing_an_in_flight_block_still_labels_the_mined_tx_confirmed() {
    let pool = isolated_pool().await;
    let block = BlockFixture::new("block-hash", 1)
        .with_txs(&[("mined", 500)])
        .build();
    let (retriever, pause) = MockBlockRetriever::with_blocks(vec![block]).pausing();
    let deps = deps(pool.clone())
        .with_cluster_retriever(MockClusterRetriever::default())
        .with_block_retriever(retriever);

    let reconciler = deps.mempool_reconciler();

    deps.mempool_ledger.assert_present(&["mined".to_string()]);
    reconciler.tick().await;

    deps.mempool_ledger.assert_absent(&["mined".to_string()]);

    let block_service = deps.block_service();
    let applying = tokio::spawn(async move {
        block_service
            .apply_block(BlockConnectedEvent {
                hash: "block-hash".to_string(),
            })
            .await;
    });

    // parked in get_block, so confirmed_at is not written yet
    pause.wait_entered().await;

    // the tick that today stamps RemoveEvicted
    reconciler.tick().await;

    pause.release();
    applying.await.expect("apply_block panicked");
    reconciler.tick().await;

    assert_eq!(
        delta_reasons_in_order(&pool, "mined").await,
        vec![DeltaReason::AddMempool, DeltaReason::RemoveConfirmed],
        "it left the mempool because it was mined, however the flush was scheduled"
    );
}

#[test]
fn a_flush_already_in_flight_when_a_block_arrives_is_relabelled_confirmed() {
    let rendered = capture(async {
        let pool = isolated_pool().await;
        let block = BlockFixture::new("block-hash", 1)
            .with_txs(&[("mined", 500)])
            .build();
        let deps = deps(pool.clone())
            .with_cluster_retriever(MockClusterRetriever::default())
            .with_block_retriever(MockBlockRetriever::with_blocks(vec![block]));
        let gate = deps.block_gate.clone();
        let reconciler = deps.mempool_reconciler();

        deps.mempool_ledger.assert_present(&["mined".to_string()]);
        reconciler.tick().await;
        deps.mempool_ledger.assert_absent(&["mined".to_string()]);

        // The pool has a single connection. Holding it parks the next flush inside
        // write_batch, after the tick has claimed the gate and before any statement.
        let held_conn = pool.get().await.expect("checkout the only connection");
        let flushing = tokio::spawn({
            let reconciler = reconciler.clone();
            async move { reconciler.tick().await }
        });
        wait_until("the flush to queue for the connection", || {
            pool.status().waiting > 0
        })
        .await;

        let block_service = deps.block_service();
        let applying = tokio::spawn(async move {
            block_service
                .apply_block(BlockConnectedEvent {
                    hash: "block-hash".to_string(),
                })
                .await;
        });
        wait_until("the block to shut the gate", || gate.is_held()).await;

        drop(held_conn);
        flushing.await.expect("tick panicked");
        applying.await.expect("apply_block panicked");
        reconciler.tick().await;

        assert_eq!(
            delta_reasons_in_order(&pool, "mined").await,
            vec![DeltaReason::AddMempool, DeltaReason::RemoveConfirmed],
            "the flush had started before the block, so it must give way and retry after it"
        );
    });

    assert_series(&rendered, "mempool_flush_preempted_total 1");
}

async fn wait_until(what: &str, condition: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !condition() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

/// Blocks are applied one after another, never concurrently, so the handover is
/// the only window that exists.
#[tokio::test]
async fn a_flush_between_two_consecutive_blocks_is_not_let_through() {
    let pool = isolated_pool().await;
    let first = BlockFixture::new("first", 1)
        .with_txs(&[("a", 500)])
        .build();
    let second = BlockFixture::new("second", 2)
        .with_txs(&[("b", 500)])
        .build();
    let (retriever, pause) = MockBlockRetriever::with_blocks(vec![first, second]).pausing();
    let deps = deps(pool.clone())
        .with_cluster_retriever(MockClusterRetriever::default())
        .with_block_retriever(retriever);

    let reconciler = deps.mempool_reconciler();
    deps.mempool_ledger
        .assert_present(&["a".to_string(), "b".to_string()]);
    reconciler.tick().await;
    deps.mempool_ledger
        .assert_absent(&["a".to_string(), "b".to_string()]);

    // shaped like `persist_blocks_and_txs`
    let block_service = deps.block_service();
    let applying = tokio::spawn(async move {
        for hash in ["first", "second"] {
            block_service
                .apply_block(BlockConnectedEvent {
                    hash: hash.to_string(),
                })
                .await;
        }
    });

    pause.wait_entered().await;
    pause.release();
    pause.wait_entered().await; // second parked, which proves the first finished

    reconciler.tick().await;

    pause.release();
    applying.await.expect("apply_block panicked");
    reconciler.tick().await;

    assert_eq!(
        delta_reasons_in_order(&pool, "a").await,
        vec![DeltaReason::AddMempool, DeltaReason::RemoveConfirmed],
        "the first block had finished, so holding the flush must not lose its tx"
    );
    assert_eq!(
        delta_reasons_in_order(&pool, "b").await,
        vec![DeltaReason::AddMempool, DeltaReason::RemoveConfirmed],
        "the second block was already in flight when the flush was attempted"
    );
}

/// Passes before the gate exists: it guards against a path that takes the gate
/// and returns without releasing it.
#[tokio::test]
async fn a_block_that_cannot_be_retrieved_does_not_strand_the_reconciler() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone())
        .with_cluster_retriever(MockClusterRetriever::default())
        .with_block_retriever(MockBlockRetriever::with_blocks(vec![]));

    // unknown hash, so apply_block bails at the retrieval step
    deps.block_service()
        .apply_block(BlockConnectedEvent {
            hash: "never-heard-of-it".to_string(),
        })
        .await;

    let reconciler = deps.mempool_reconciler();
    deps.mempool_ledger.assert_present(&["a".to_string()]);
    reconciler.tick().await;

    assert_eq!(
        delta_reasons_in_order(&pool, "a").await,
        vec![DeltaReason::AddMempool],
        "apply_block did no work, so nothing may hold the reconciler afterwards"
    );
}

/// Holding a flush must postpone it, never discard it: the journal drops silently
/// past `JOURNAL_CAP` and only records that it went degraded.
#[tokio::test]
async fn entries_journalled_while_a_block_is_in_flight_are_not_lost() {
    let pool = isolated_pool().await;
    let block = BlockFixture::new("block-hash", 1)
        .with_txs(&[("mined", 500)])
        .build();
    let (retriever, pause) = MockBlockRetriever::with_blocks(vec![block]).pausing();
    let deps = deps(pool.clone())
        .with_cluster_retriever(MockClusterRetriever::default())
        .with_block_retriever(retriever);

    let reconciler = deps.mempool_reconciler();
    let block_service = deps.block_service();
    let applying = tokio::spawn(async move {
        block_service
            .apply_block(BlockConnectedEvent {
                hash: "block-hash".to_string(),
            })
            .await;
    });
    pause.wait_entered().await;

    let arrivals: Vec<String> = (0..2_000).map(|i| format!("arrival-{i}")).collect();
    deps.mempool_ledger.assert_present(&arrivals);

    for _ in 0..5 {
        reconciler.tick().await;
    }

    pause.release();
    applying.await.expect("apply_block panicked");
    reconciler.tick().await;

    assert!(
        !deps.mempool_ledger.is_degraded(),
        "the journal must not overflow while a block holds the flush back"
    );
    assert_eq!(deps.mempool_ledger.pending_len(), 0, "everything drained");

    let mut conn = pool.get().await.expect("checkout connection");
    let written: i64 = mempool_deltas::table
        .filter(mempool_deltas::txid.like("arrival-%"))
        .filter(mempool_deltas::reason.eq(DeltaReason::AddMempool))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count arrivals");
    assert_eq!(
        written, 2_000,
        "every arrival journalled during the block must still reach the table"
    );
}

/// `CounterSampleService` buckets the txs/min chart by `created_at`, which
/// defaults to `now()` and so records the flush, not the sighting.
///
/// Relative because `isolated_pool` holds one open transaction, freezing `now()`
/// so every row of a test shares a stamp. An absolute assertion would pass here
/// for the wrong reason.
#[tokio::test]
async fn two_txs_seen_seconds_apart_do_not_share_one_flush_timestamp() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone()).with_cluster_retriever(MockClusterRetriever::default());
    let reconciler = deps.mempool_reconciler();

    deps.mempool_ledger.assert_present(&["early".to_string()]);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    deps.mempool_ledger.assert_present(&["late".to_string()]);

    // one flush carries both, as it would after a block held the tick back
    reconciler.tick().await;

    let early = delta_created_at(&pool, "early").await;
    let late = delta_created_at(&pool, "late").await;
    // The sleep is monotonic but the stamps are wall clock, which the host's
    // time sync can step backwards, so the threshold sits well under the sleep
    // rather than just under it.
    assert!(
        late - early >= time::Duration::seconds(1),
        "seen 2s apart, stamped {early} and {late}: a row must carry the sighting"
    );
}
