#![cfg(feature = "db_integration_tests")]

use api::db::models::{BackfillStage, Transaction};
use api::infra::deps::Deps;
use api::services::tx_backfill::{BackfillRequest, TxBackfillConsumer, TxBackfillQueue};
use testkit::deps::deps;
use testkit::fixtures::{MempoolEntryFixture, TX_FEE, TX_VSIZE, TxFixture};
use testkit::metrics::{assert_no_series, assert_series, local_recorder};
use testkit::mocks::MockTransactionRetriever;
use testkit::postgres::isolated_pool;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, mpsc};

async fn stored_tx(repo: &api::db::TransactionRepository, txid: &str) -> Transaction {
    repo.find_by_txids(&[txid.to_string()])
        .await
        .expect("query")
        .pop()
        .expect("row exists")
}

fn consumer_for(
    deps: &Deps<MockTransactionRetriever>,
) -> (
    TxBackfillQueue,
    mpsc::Receiver<BackfillRequest>,
    TxBackfillConsumer<MockTransactionRetriever>,
) {
    let queue = TxBackfillQueue::new(8);
    let rx = queue.take_receiver().expect("receiver");
    let consumer = TxBackfillConsumer::new(
        deps.repos.transaction.clone(),
        deps.transaction_retriever.clone(),
        queue.clone(),
        deps.mempool_ledger.clone(),
    );
    (queue, rx, consumer)
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
    queue.enqueue("txa".to_string(), BackfillStage::Raw);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    let shutdown = Arc::new(Notify::new());
    shutdown.notify_one();
    consumer.consume(rx, shutdown).await;
    drop(guard);

    let row = stored_tx(&deps.repos.transaction, "txa").await;
    assert_eq!(row.input_txids, Some(vec!["parent-of-txa".to_string()]));
    assert_eq!(row.vsize, 141);
    assert_eq!(
        row.fee,
        Some(TX_FEE),
        "the entry stage ran right after the raw one"
    );
    assert!(row.is_complete());

    let rendered = handle.render();
    assert_series(&rendered, r#"tx_backfill_total{stage="raw"} 1"#);
    assert_series(&rendered, r#"tx_backfill_total{stage="entry"} 1"#);
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
    queue.enqueue("txd".to_string(), BackfillStage::Raw);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    let shutdown = Arc::new(Notify::new());
    shutdown.notify_one();
    consumer.consume(rx, shutdown).await;
    drop(guard);

    // the guard in `backfill_raw` makes the late write a no-op
    let input_txids = stored_tx(&deps.repos.transaction, "txd").await.input_txids;
    assert_eq!(input_txids, Some(vec!["already-known".to_string()]));
    assert_no_series(&handle.render(), "tx_backfill_total");
    assert!(
        mock.entries_fetched().is_empty(),
        "a row the raw stage found complete owes no entry stage"
    );
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
        queue.enqueue(txid.to_string(), BackfillStage::Raw);
    }

    // shutdown is requested before `consume` ever starts draining
    let shutdown = Arc::new(Notify::new());
    shutdown.notify_one();
    consumer.consume(rx, shutdown).await;

    for txid in ["a", "b", "c"] {
        let row = stored_tx(&deps.repos.transaction, txid).await;
        assert_eq!(row.input_txids, Some(vec![format!("parent-of-{txid}")]));
        assert!(row.is_complete());
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
    queue.enqueue("fails".to_string(), BackfillStage::Raw);
    queue.enqueue("ok".to_string(), BackfillStage::Raw);

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

    let fails = stored_tx(&deps.repos.transaction, "fails").await;
    assert_eq!(fails.input_txids, None);

    let ok = stored_tx(&deps.repos.transaction, "ok").await;
    assert_eq!(ok.input_txids, Some(vec!["parent-of-ok".to_string()]));
    assert!(ok.is_complete());
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
    queue.enqueue("fails".to_string(), BackfillStage::Raw);

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

    let fails_input_txids = stored_tx(&deps.repos.transaction, "fails")
        .await
        .input_txids;
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
    queue.enqueue("bad".to_string(), BackfillStage::Raw);

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

    let input_txids = stored_tx(&deps.repos.transaction, "bad").await.input_txids;
    assert_eq!(input_txids, None);
}

#[tokio::test]
async fn consume_at_the_entry_stage_fetches_only_the_fee() {
    let pool = isolated_pool().await;
    let mock = MockTransactionRetriever::default();
    let deps = deps(pool.clone()).with_transaction_retriever(mock.clone());
    deps.repos
        .transaction
        .insert(
            &TxFixture::new("txe")
                .with_vsize(TX_VSIZE)
                .with_input_txids(&["parent"])
                .build(),
        )
        .await
        .expect("seed row missing only the fee");

    let (queue, rx, consumer) = consumer_for(&deps);
    queue.enqueue("txe".to_string(), BackfillStage::Entry);

    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    let shutdown = Arc::new(Notify::new());
    shutdown.notify_one();
    consumer.consume(rx, shutdown).await;
    drop(guard);

    assert!(mock.txs_fetched().is_empty(), "no getrawtransaction");
    assert_eq!(mock.entries_fetched(), vec!["txe".to_string()]);

    let row = stored_tx(&deps.repos.transaction, "txe").await;
    assert_eq!(row.fee, Some(TX_FEE));
    assert_eq!(row.input_txids, Some(vec!["parent".to_string()]));
    assert!(row.is_complete());

    let rendered = handle.render();
    assert_series(&rendered, r#"tx_backfill_total{stage="entry"} 1"#);
    assert_no_series(&rendered, r#"tx_backfill_total{stage="raw"}"#);
}

#[tokio::test]
async fn consume_skips_the_entry_stage_when_the_fee_is_already_known() {
    let pool = isolated_pool().await;
    let mock = MockTransactionRetriever::default();
    let deps = deps(pool.clone()).with_transaction_retriever(mock.clone());
    // what bootstrap writes: fee and vsize from the verbose entry, no parents
    deps.repos
        .transaction
        .insert(&api::db::models::NewTransaction::from(
            &MempoolEntryFixture::new("txb").build(),
        ))
        .await
        .expect("seed bootstrap row");

    let (queue, rx, consumer) = consumer_for(&deps);
    queue.enqueue("txb".to_string(), BackfillStage::Raw);

    let shutdown = Arc::new(Notify::new());
    shutdown.notify_one();
    consumer.consume(rx, shutdown).await;

    assert_eq!(mock.txs_fetched(), vec!["txb".to_string()]);
    assert!(mock.entries_fetched().is_empty(), "no getmempoolentry");

    let row = stored_tx(&deps.repos.transaction, "txb").await;
    assert_eq!(row.input_txids, Some(vec!["parent-of-txb".to_string()]));
    assert!(row.is_complete());
}

#[tokio::test]
async fn consume_keeps_the_raw_stage_when_the_tx_leaves_before_the_entry_stage() {
    let pool = isolated_pool().await;
    let mock = MockTransactionRetriever::entry_not_found_for(["gone".to_string()]);
    let deps = deps(pool.clone()).with_transaction_retriever(mock.clone());
    deps.repos
        .transaction
        .insert(&TxFixture::new("gone").build())
        .await
        .expect("seed hollow row");

    let (queue, rx, consumer) = consumer_for(&deps);
    queue.enqueue("gone".to_string(), BackfillStage::Raw);

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

    assert_eq!(
        mock.entries_fetched(),
        vec!["gone".to_string()],
        "terminal, never retried"
    );

    let row = stored_tx(&deps.repos.transaction, "gone").await;
    assert_eq!(row.input_txids, Some(vec!["parent-of-gone".to_string()]));
    assert_eq!(row.fee, None);
    assert!(
        !row.is_complete(),
        "a row that never got its fee stays incomplete"
    );
}

#[tokio::test]
async fn consume_retries_a_failed_entry_stage_without_refetching_the_raw_tx() {
    let pool = isolated_pool().await;
    let mock = MockTransactionRetriever::entry_failing_for(["flaky".to_string()]);
    let deps = deps(pool.clone()).with_transaction_retriever(mock.clone());
    deps.repos
        .transaction
        .insert(&TxFixture::new("flaky").build())
        .await
        .expect("seed hollow row");

    let (queue, rx, consumer) = consumer_for(&deps);
    queue.enqueue("flaky".to_string(), BackfillStage::Raw);

    let shutdown = Arc::new(Notify::new());
    let shutdown_for_consumer = shutdown.clone();
    let handle = tokio::spawn(async move { consumer.consume(rx, shutdown_for_consumer).await });

    // entry fetches land at t = 0, 0.25s, 0.75s; the 4th at 1.75s is past the shutdown
    tokio::time::sleep(Duration::from_millis(1200)).await;
    shutdown.notify_one();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("consumer drained within the timeout")
        .expect("consumer task did not panic");

    assert_eq!(
        mock.txs_fetched(),
        vec!["flaky".to_string()],
        "raw fetched once"
    );
    assert_eq!(mock.entries_fetched().len(), 3);

    let row = stored_tx(&deps.repos.transaction, "flaky").await;
    assert_eq!(
        row.input_txids,
        Some(vec!["parent-of-flaky".to_string()]),
        "the raw stage's write survives the entry stage failing"
    );
    assert_eq!(row.fee, None);
}

// endregion: consumer
