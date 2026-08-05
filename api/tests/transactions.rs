#![cfg(feature = "db_integration_tests")]

use api::db::TransactionRepository;
use api::db::models::NewTransaction;
use api::db::schema::transactions;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use testkit::fixtures::RawTxFixture;
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

#[tokio::test]
async fn existing_txids_finds_inserted_and_ignores_duplicates() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    let tx = NewTransaction::from(&RawTxFixture::new("deadbeef").build());
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
async fn first_seen_at_is_persisted_as_our_clock() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool.clone());

    let raw = RawTxFixture::new("deadbeef")
        .with_time(Some(OffsetDateTime::UNIX_EPOCH))
        .build();
    let before = OffsetDateTime::now_utc();
    repo.insert(&NewTransaction::from(&raw))
        .await
        .expect("insert");

    let mut conn = pool.get().await.expect("conn");
    let stored: OffsetDateTime = transactions::table
        .filter(transactions::txid.eq("deadbeef"))
        .select(transactions::first_seen_at)
        .first(&mut conn)
        .await
        .expect("load first_seen_at");

    assert!(stored >= before);
}
