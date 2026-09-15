#![cfg(feature = "db_integration_tests")]

use api::db::schema::transactions;
use api::services::tx_backfill::{TxBackfillConsumer, TxBackfillQueue};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use testkit::deps::deps;
use testkit::fixtures::TxFixture;
use testkit::mocks::MockTransactionRetriever;
use testkit::postgres::isolated_pool;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

async fn stored_row(pool: &api::db::DbPool, txid: &str) -> (Option<Vec<String>>, i64, bool) {
    let mut conn = pool.get().await.expect("conn");
    transactions::table
        .filter(transactions::txid.eq(txid))
        .select((
            transactions::input_txids,
            transactions::vsize,
            transactions::hollow,
        ))
        .first(&mut conn)
        .await
        .expect("load row")
}

// region: consumer

#[tokio::test]
async fn consume_backfills_a_queued_hollow_row() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone()).with_transaction_retriever(MockTransactionRetriever::default());
    deps.repos
        .transaction
        .insert(&TxFixture::new("txa").build())
        .await
        .expect("seed hollow row");

    let queue = TxBackfillQueue::new(8);
    let rx = queue.take_receiver().expect("receiver");
    let consumer = TxBackfillConsumer::new(
        deps.repos.transaction.clone(),
        deps.transaction_retriever.clone(),
        queue.clone(),
        deps.mempool_ledger.clone(),
    );
    queue.enqueue("txa".to_string());

    let shutdown = Arc::new(Notify::new());
    shutdown.notify_one();
    consumer.consume(rx, shutdown).await;

    let (input_txids, vsize, hollow) = stored_row(&pool, "txa").await;
    assert_eq!(input_txids, Some(vec!["parent-of-txa".to_string()]));
    assert_eq!(vsize, 141);
    assert!(!hollow);
}

#[tokio::test]
async fn consume_leaves_row_untouched_when_it_already_has_parents() {
    let pool = isolated_pool().await;
    let mock = MockTransactionRetriever::default();
    let deps = deps(pool.clone()).with_transaction_retriever(mock.clone());
    deps.repos
        .transaction
        .insert(
            &TxFixture::new("txd")
                .sized()
                .with_input_txids(&["already-known"])
                .build(),
        )
        .await
        .expect("seed row with parents");

    let queue = TxBackfillQueue::new(8);
    let rx = queue.take_receiver().expect("receiver");
    let consumer = TxBackfillConsumer::new(
        deps.repos.transaction.clone(),
        deps.transaction_retriever.clone(),
        queue.clone(),
        deps.mempool_ledger.clone(),
    );
    queue.enqueue("txd".to_string());

    let shutdown = Arc::new(Notify::new());
    shutdown.notify_one();
    consumer.consume(rx, shutdown).await;

    // the guard in `backfill_from_fetch` makes the late write a no-op
    let (input_txids, _, _) = stored_row(&pool, "txd").await;
    assert_eq!(input_txids, Some(vec!["already-known".to_string()]));
}

#[tokio::test]
async fn consume_drains_the_full_backlog_buffered_before_shutdown() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone()).with_transaction_retriever(MockTransactionRetriever::default());
    let seed: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|txid| TxFixture::new(txid).build())
        .collect();
    deps.repos
        .transaction
        .insert_many(&seed)
        .await
        .expect("seed rows");

    let queue = TxBackfillQueue::new(8);
    let rx = queue.take_receiver().expect("receiver");
    let consumer = TxBackfillConsumer::new(
        deps.repos.transaction.clone(),
        deps.transaction_retriever.clone(),
        queue.clone(),
        deps.mempool_ledger.clone(),
    );
    for txid in ["a", "b", "c"] {
        queue.enqueue(txid.to_string());
    }

    // shutdown is requested before `consume` ever starts draining
    let shutdown = Arc::new(Notify::new());
    shutdown.notify_one();
    consumer.consume(rx, shutdown).await;

    for txid in ["a", "b", "c"] {
        let (input_txids, _, hollow) = stored_row(&pool, txid).await;
        assert_eq!(input_txids, Some(vec![format!("parent-of-{txid}")]));
        assert!(!hollow);
    }
}

#[tokio::test]
async fn consume_retries_a_transient_failure_without_blocking_others() {
    let pool = isolated_pool().await;
    let mock = MockTransactionRetriever::failing_for(["fails".to_string()]);
    let deps = deps(pool.clone()).with_transaction_retriever(mock.clone());
    deps.repos
        .transaction
        .insert(&TxFixture::new("fails").build())
        .await
        .expect("seed failing row");
    deps.repos
        .transaction
        .insert(&TxFixture::new("ok").build())
        .await
        .expect("seed ok row");

    let queue = TxBackfillQueue::new(8);
    let rx = queue.take_receiver().expect("receiver");
    let consumer = TxBackfillConsumer::new(
        deps.repos.transaction.clone(),
        deps.transaction_retriever.clone(),
        queue.clone(),
        deps.mempool_ledger.clone(),
    );
    queue.enqueue("fails".to_string());
    queue.enqueue("ok".to_string());

    let shutdown = Arc::new(Notify::new());
    let shutdown_for_consumer = shutdown.clone();
    let handle = tokio::spawn(async move { consumer.consume(rx, shutdown_for_consumer).await });

    // "fails" is absent from the empty mempool ledger, so MAX_ATTEMPTS applies.
    // Give the two backoffs (250ms + 500ms) time to run out before shutting down.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    shutdown.notify_one();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("consumer drained within the timeout")
        .expect("consumer task did not panic");

    // the backoff schedule puts the 4th fetch at t = 1.75s, past the shutdown
    let fails_attempts = mock
        .txs_fetched()
        .into_iter()
        .filter(|t| t == "fails")
        .count();
    assert_eq!(fails_attempts, 3);

    let (fails_input_txids, _, _) = stored_row(&pool, "fails").await;
    assert_eq!(fails_input_txids, None);

    let (ok_input_txids, _, hollow) = stored_row(&pool, "ok").await;
    assert_eq!(ok_input_txids, Some(vec!["parent-of-ok".to_string()]));
    assert!(!hollow);
}

#[tokio::test]
async fn consume_abandons_an_in_flight_retry_backoff_on_shutdown() {
    let pool = isolated_pool().await;
    let mock = MockTransactionRetriever::failing_for(["fails".to_string()]);
    let deps = deps(pool.clone()).with_transaction_retriever(mock.clone());
    deps.repos
        .transaction
        .insert(&TxFixture::new("fails").build())
        .await
        .expect("seed failing row");

    // the node still has it, so no attempt cap applies
    deps.mempool_ledger
        .seed(HashSet::from(["fails".to_string()]));

    let queue = TxBackfillQueue::new(8);
    let rx = queue.take_receiver().expect("receiver");
    let consumer = TxBackfillConsumer::new(
        deps.repos.transaction.clone(),
        deps.transaction_retriever.clone(),
        queue.clone(),
        deps.mempool_ledger.clone(),
    );
    queue.enqueue("fails".to_string());

    let shutdown = Arc::new(Notify::new());
    let shutdown_for_consumer = shutdown.clone();
    let handle = tokio::spawn(async move { consumer.consume(rx, shutdown_for_consumer).await });

    // backoff doubles from 250ms, so fetches land at t = 0, 0.25s, 0.75s, 1.75s
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // at this point the 4th failure is ~1.75s into a 2s backoff; shutdown must
    // abandon it rather than wait it out
    let requested_at = Instant::now();
    shutdown.notify_one();
    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("consumer drained within the timeout")
        .expect("consumer task did not panic");
    assert!(
        requested_at.elapsed() < Duration::from_millis(750),
        "shutdown waited out the retry backoff: {:?}",
        requested_at.elapsed()
    );

    // a ledger-absent tx would have stopped at MAX_ATTEMPTS; this one keeps going
    let fails_attempts = mock
        .txs_fetched()
        .into_iter()
        .filter(|t| t == "fails")
        .count();
    assert!(
        fails_attempts >= 4,
        "expected the in-mempool branch to keep retrying, got {fails_attempts} fetches"
    );

    let (fails_input_txids, _, _) = stored_row(&pool, "fails").await;
    assert_eq!(fails_input_txids, None);
}

#[tokio::test]
async fn consume_gives_up_immediately_on_a_malformed_txid() {
    let pool = isolated_pool().await;
    let mock = MockTransactionRetriever::invalid_for(["bad".to_string()]);
    let deps = deps(pool.clone()).with_transaction_retriever(mock.clone());
    deps.repos
        .transaction
        .insert(&TxFixture::new("bad").build())
        .await
        .expect("seed malformed row");

    let queue = TxBackfillQueue::new(8);
    let rx = queue.take_receiver().expect("receiver");
    let consumer = TxBackfillConsumer::new(
        deps.repos.transaction.clone(),
        deps.transaction_retriever.clone(),
        queue.clone(),
        deps.mempool_ledger.clone(),
    );
    queue.enqueue("bad".to_string());

    let shutdown = Arc::new(Notify::new());
    let shutdown_for_consumer = shutdown.clone();
    let handle = tokio::spawn(async move { consumer.consume(rx, shutdown_for_consumer).await });

    // well past the 250ms first backoff a retry would have taken
    tokio::time::sleep(Duration::from_millis(800)).await;
    shutdown.notify_one();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("consumer drained within the timeout")
        .expect("consumer task did not panic");

    let attempts = mock
        .txs_fetched()
        .into_iter()
        .filter(|t| t == "bad")
        .count();
    assert_eq!(attempts, 1, "a txid that cannot parse must not be retried");

    let (input_txids, _, _) = stored_row(&pool, "bad").await;
    assert_eq!(input_txids, None);
}

// endregion: consumer
