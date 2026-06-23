//! `persist_deltas_and_new_txs` stream-consumer integration tests.
#![cfg(feature = "db_integration_tests")]

mod common;

use api::db::models::NewTransaction;
use api::db::{MempoolDeltaRepository, TransactionRepository};
use api::services::mempool::persist_deltas_and_new_txs;
use common::dummy_tx;
use shared::events::MempoolDeltaEvent;
use testkit::mocks::MockTransactionRetriever;
use testkit::postgres::isolated_pool;

#[tokio::test]
async fn streamed_deltas_are_persisted() {
    let pool = isolated_pool().await;
    let mempool_repo = MempoolDeltaRepository::new(pool.clone());
    let transaction_repo = TransactionRepository::new(pool);

    let retriever = MockTransactionRetriever::default();

    let source = futures::stream::iter(vec![
        MempoolDeltaEvent {
            added: vec!["a".into()],
            removed: vec![],
        },
        MempoolDeltaEvent {
            added: vec![],
            removed: vec!["b".into()],
        },
    ]);

    persist_deltas_and_new_txs(mempool_repo.clone(), transaction_repo, retriever, source).await;

    assert_eq!(mempool_repo.count().await.expect("count deltas"), 2);
}

#[tokio::test]
async fn persist_deltas_fetches_and_stores_new_transactions() {
    let pool = isolated_pool().await;
    let mempool_repo = MempoolDeltaRepository::new(pool.clone());
    let transaction_repo = TransactionRepository::new(pool);

    let retriever = MockTransactionRetriever::default();

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["tx1".into(), "tx2".into()],
        removed: vec![],
    }]);

    persist_deltas_and_new_txs(
        mempool_repo,
        transaction_repo.clone(),
        retriever.clone(),
        source,
    )
    .await;

    assert_eq!(
        retriever.txs_fetched(),
        vec!["tx1".to_string(), "tx2".to_string()]
    );

    let mut stored = transaction_repo
        .existing_txids(&["tx1".to_string(), "tx2".to_string()])
        .await
        .expect("query existing");
    stored.sort();
    assert_eq!(stored, vec!["tx1".to_string(), "tx2".to_string()]);
}

#[tokio::test]
async fn persist_deltas_skips_already_known_transactions() {
    let pool = isolated_pool().await;
    let mempool_repo = MempoolDeltaRepository::new(pool.clone());
    let transaction_repo = TransactionRepository::new(pool);

    transaction_repo
        .insert(&NewTransaction::from(&dummy_tx("known")))
        .await
        .expect("seed known tx");

    let retriever = MockTransactionRetriever::default();

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["known".into(), "fresh".into()],
        removed: vec![],
    }]);

    persist_deltas_and_new_txs(
        mempool_repo,
        transaction_repo.clone(),
        retriever.clone(),
        source,
    )
    .await;

    assert_eq!(retriever.txs_fetched(), vec!["fresh".to_string()]);

    let mut stored = transaction_repo
        .existing_txids(&["known".to_string(), "fresh".to_string()])
        .await
        .expect("query existing");
    stored.sort();
    assert_eq!(stored, vec!["fresh".to_string(), "known".to_string()]);
}
