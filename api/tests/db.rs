#![cfg(feature = "db_integration_tests")]

use api::db::models::NewTransaction;
use api::db::{MempoolDeltaRepository, TransactionRepository};
use api::services::mempool::persist_deltas;
use shared::events::MempoolDeltaEvent;
use shared::models::GetRawTransactionModel;
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

fn dummy_tx(txid: &str) -> GetRawTransactionModel {
    GetRawTransactionModel {
        txid: txid.to_string(),
        version: 2,
        lock_time: u32::MAX,
        vsize: 141,
        weight: 561,
        input_count: 1,
        input_txids: vec!["parent-a".into(), "parent-b".into()],
        output_count: 2,
        confirmations: 0,
        time: Some(OffsetDateTime::UNIX_EPOCH),
    }
}

#[tokio::test]
async fn existing_txids_finds_inserted_and_ignores_duplicates() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    let tx = NewTransaction::from(&dummy_tx("deadbeef"));
    let inserted = repo.insert(&tx).await.expect("insert");
    assert_eq!(inserted, 1);

    let found = repo
        .existing_txids(&["deadbeef".to_string(), "missing".to_string()])
        .await
        .expect("query existing");
    assert_eq!(found, vec!["deadbeef".to_string()]);

    // on conflict do nothing
    let again = repo.insert(&tx).await.expect("re-insert");
    assert_eq!(again, 0);
}

#[tokio::test]
async fn streamed_deltas_are_persisted() {
    let repo = MempoolDeltaRepository::new(isolated_pool().await);

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

    // The background persister drains the stream, writing every delta.
    persist_deltas(repo.clone(), source).await;

    assert_eq!(repo.count().await.expect("count deltas"), 2);
}
