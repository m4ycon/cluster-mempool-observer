#![cfg(feature = "db_integration_tests")]

mod common;

use api::db::TransactionRepository;
use api::db::models::NewTransaction;
use api::db::schema::transactions;
use common::dummy_tx;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

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
async fn first_seen_at_is_persisted_non_null() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool.clone());

    let raw = dummy_tx("deadbeef");
    let expected = raw.time.expect("dummy has time");
    let tx = NewTransaction::from(&raw);
    repo.insert(&tx).await.expect("insert");

    let mut conn = pool.get().await.expect("conn");
    let stored: OffsetDateTime = transactions::table
        .filter(transactions::txid.eq("deadbeef"))
        .select(transactions::first_seen_at)
        .first(&mut conn)
        .await
        .expect("load first_seen_at");

    assert_eq!(stored, expected);
}

#[tokio::test]
async fn first_seen_at_filled_when_source_time_missing() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool.clone());

    let mut raw = dummy_tx("cafebabe");
    raw.time = None;
    let before = OffsetDateTime::now_utc();
    let tx = NewTransaction::from(&raw);
    repo.insert(&tx).await.expect("insert");

    let mut conn = pool.get().await.expect("conn");
    let stored: OffsetDateTime = transactions::table
        .filter(transactions::txid.eq("cafebabe"))
        .select(transactions::first_seen_at)
        .first(&mut conn)
        .await
        .expect("load first_seen_at");

    assert!(stored >= before);
}
