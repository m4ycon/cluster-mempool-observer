#![cfg(feature = "db_integration_tests")]

use api::db::TransactionRepository;
use api::db::models::NewTransaction;
use api::db::schema::transactions;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use testkit::fixtures::{RawTxFixture, TxFixture, seed_txs};
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
async fn find_by_txids_returns_only_rows_that_exist() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);
    seed_txs(&repo, &["a", "b", "c"]).await;

    let found = repo
        .find_by_txids(&[
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "missing".to_string(),
        ])
        .await
        .expect("query by txids");

    let mut txids: Vec<String> = found.into_iter().map(|tx| tx.txid).collect();
    txids.sort();
    assert_eq!(
        txids,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
}

#[tokio::test]
async fn find_by_txids_preserves_the_none_vs_empty_input_txids_distinction() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    repo.insert(&TxFixture::new("unknown").build())
        .await
        .expect("insert unknown-ancestry tx");
    repo.insert(&TxFixture::new("coinbase").with_input_txids(&[]).build())
        .await
        .expect("insert coinbase tx");
    repo.insert(&TxFixture::new("child").with_input_txids(&["p"]).build())
        .await
        .expect("insert tx with a parent");

    let found = repo
        .find_by_txids(&[
            "unknown".to_string(),
            "coinbase".to_string(),
            "child".to_string(),
        ])
        .await
        .expect("query by txids");

    let by_txid: std::collections::HashMap<String, Option<Vec<String>>> = found
        .into_iter()
        .map(|tx| (tx.txid, tx.input_txids))
        .collect();

    assert_eq!(by_txid["unknown"], None);
    assert_eq!(by_txid["coinbase"], Some(vec![]));
    assert_eq!(by_txid["child"], Some(vec!["p".to_string()]));
}

#[tokio::test]
async fn find_by_txids_on_an_empty_slice_returns_no_rows() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);
    seed_txs(&repo, &["a", "b"]).await;

    let found = repo.find_by_txids(&[]).await.expect("query empty slice");

    assert!(found.is_empty());
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

#[tokio::test]
async fn backfill_from_fetch_sets_parents_and_vsize_without_touching_fee() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool.clone());

    repo.insert(&TxFixture::new("deadbeef").build())
        .await
        .expect("insert hollow tx");

    let updated = repo
        .backfill_from_fetch(
            "deadbeef",
            &["parent-a".to_string(), "parent-b".to_string()],
            200,
        )
        .await
        .expect("backfill inputs");
    assert_eq!(updated, 1);

    let (input_txids, vsize, hollow, fee): (Option<Vec<String>>, i64, bool, Option<i64>) = {
        let mut conn = pool.get().await.expect("conn");
        transactions::table
            .filter(transactions::txid.eq("deadbeef"))
            .select((
                transactions::input_txids,
                transactions::vsize,
                transactions::hollow,
                transactions::fee,
            ))
            .first(&mut conn)
            .await
            .expect("load backfilled row")
    };

    assert_eq!(
        input_txids,
        Some(vec!["parent-a".to_string(), "parent-b".to_string()])
    );
    assert_eq!(vsize, 200);
    assert!(!hollow);
    assert_eq!(fee, None);
}

#[tokio::test]
async fn backfill_from_fetch_is_a_no_op_when_row_already_has_parents() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool.clone());

    repo.insert(
        &TxFixture::new("deadbeef")
            .sized()
            .with_input_txids(&["already-known"])
            .with_fee(Some(500))
            .build(),
    )
    .await
    .expect("insert confirmed tx with parents");

    let updated = repo
        .backfill_from_fetch("deadbeef", &["late-parent".to_string()], 999)
        .await
        .expect("backfill inputs");
    assert_eq!(updated, 0);

    let (input_txids, vsize, fee): (Option<Vec<String>>, i64, Option<i64>) = {
        let mut conn = pool.get().await.expect("conn");
        transactions::table
            .filter(transactions::txid.eq("deadbeef"))
            .select((
                transactions::input_txids,
                transactions::vsize,
                transactions::fee,
            ))
            .first(&mut conn)
            .await
            .expect("load untouched row")
    };

    assert_eq!(input_txids, Some(vec!["already-known".to_string()]));
    assert_eq!(vsize, testkit::fixtures::TX_VSIZE);
    assert_eq!(fee, Some(500));
}
