#![cfg(feature = "db_integration_tests")]

mod common;

use api::db::models::NewTransaction;
use api::db::{MempoolDeltaRepository, TransactionRepository};
use api::services::mempool::MempoolService;
use api::services::pubsub::PubSubService;
use common::dummy_tx;
use shared::events::MempoolDeltaEvent;
use shared::pubsub::PubSub;
use testkit::mocks::MockTransactionRetriever;
use testkit::postgres::isolated_pool;

#[tokio::test]
async fn streamed_deltas_are_persisted() {
    let pool = isolated_pool().await;
    let mempool_delta_repo = MempoolDeltaRepository::new(pool.clone());
    let mempool_service = MempoolService::new(
        mempool_delta_repo.clone(),
        TransactionRepository::new(pool),
        MockTransactionRetriever::default(),
        PubSubService::new(PubSub::new()),
    );

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

    mempool_service.persist_deltas_and_new_txs(source).await;

    assert_eq!(mempool_delta_repo.count().await.expect("count deltas"), 2);
}

#[tokio::test]
async fn persist_deltas_fetches_and_stores_new_transactions() {
    let pool = isolated_pool().await;
    let transaction_repo = TransactionRepository::new(pool.clone());
    let mock_transaction_retriever = MockTransactionRetriever::default();
    let mempool_service = MempoolService::new(
        MempoolDeltaRepository::new(pool),
        transaction_repo.clone(),
        mock_transaction_retriever.clone(),
        PubSubService::new(PubSub::new()),
    );

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["tx1".into(), "tx2".into()],
        removed: vec![],
    }]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    assert_eq!(
        mock_transaction_retriever.txs_fetched(),
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
    let transaction_repo = TransactionRepository::new(pool.clone());
    let mock_transaction_retriever = MockTransactionRetriever::default();
    let mempool_service = MempoolService::new(
        MempoolDeltaRepository::new(pool),
        transaction_repo.clone(),
        mock_transaction_retriever.clone(),
        PubSubService::new(PubSub::new()),
    );

    transaction_repo
        .insert(&NewTransaction::from(&dummy_tx("known")))
        .await
        .expect("seed known tx");

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["known".into(), "fresh".into()],
        removed: vec![],
    }]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    assert_eq!(
        mock_transaction_retriever.txs_fetched(),
        vec!["fresh".to_string()]
    );

    let mut stored = transaction_repo
        .existing_txids(&["known".to_string(), "fresh".to_string()])
        .await
        .expect("query existing");
    stored.sort();
    assert_eq!(stored, vec!["fresh".to_string(), "known".to_string()]);
}
