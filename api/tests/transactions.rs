#![cfg(feature = "db_integration_tests")]

use api::db::models::{BackfillStage, NewTransaction, Transaction};
use api::db::schema::transactions;
use api::db::{TRANSACTION_INSERT_CHUNK_SIZE, TransactionRepository};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use testkit::fixtures::{MempoolEntryFixture, RawTxFixture, TX_FEE, TX_VSIZE, TxFixture, seed_txs};
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

async fn stored(repo: &TransactionRepository, txid: &str) -> Transaction {
    repo.find_by_txids(&[txid.to_string()])
        .await
        .expect("query")
        .pop()
        .expect("row exists")
}

#[tokio::test]
async fn backfill_raw_sets_parents_and_vsize_and_reports_the_fee_still_owed() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    repo.insert(&TxFixture::new("deadbeef").build())
        .await
        .expect("insert hollow tx");

    let fill = repo
        .backfill_raw(
            "deadbeef",
            &["parent-a".to_string(), "parent-b".to_string()],
            200,
        )
        .await
        .expect("backfill raw")
        .expect("the hollow row was filled");
    assert_eq!(fill.backfill_stage(), Some(BackfillStage::Entry));

    let row = stored(&repo, "deadbeef").await;
    assert_eq!(
        row.input_txids,
        Some(vec!["parent-a".to_string(), "parent-b".to_string()])
    );
    assert_eq!(row.vsize, 200);
    assert_eq!(row.fee, None);
    assert!(!row.is_complete(), "incomplete while the fee is missing");
}

#[tokio::test]
async fn backfill_raw_completes_a_row_whose_fee_is_already_known() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    // what bootstrap writes: fee and vsize from the verbose entry, no parents
    repo.insert(&NewTransaction::from(
        &MempoolEntryFixture::new("deadbeef").build(),
    ))
    .await
    .expect("insert bootstrap row");

    let fill = repo
        .backfill_raw("deadbeef", &["parent".to_string()], 200)
        .await
        .expect("backfill raw")
        .expect("the row lacking parents was filled");
    assert_eq!(fill.backfill_stage(), None);

    let row = stored(&repo, "deadbeef").await;
    assert_eq!(row.fee, Some(TX_FEE));
    assert!(row.is_complete());
}

#[tokio::test]
async fn backfill_raw_is_a_no_op_when_row_already_has_parents() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    repo.insert(
        &TxFixture::new("deadbeef")
            .sized()
            .with_input_txids(&["already-known"])
            .with_fee(Some(500))
            .build(),
    )
    .await
    .expect("insert confirmed tx with parents");

    let fill = repo
        .backfill_raw("deadbeef", &["late-parent".to_string()], 999)
        .await
        .expect("backfill raw");
    assert_eq!(fill, None);

    let row = stored(&repo, "deadbeef").await;
    assert_eq!(row.input_txids, Some(vec!["already-known".to_string()]));
    assert_eq!(row.vsize, TX_VSIZE);
    assert_eq!(row.fee, Some(500));
}

#[tokio::test]
async fn backfill_fee_completes_a_row_that_already_has_parents_and_vsize() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    repo.insert(
        &TxFixture::new("deadbeef")
            .with_vsize(TX_VSIZE)
            .with_input_txids(&["parent"])
            .build(),
    )
    .await
    .expect("insert row missing only the fee");

    let fill = repo
        .backfill_fee("deadbeef", 1234)
        .await
        .expect("backfill fee")
        .expect("the row lacking a fee was filled");
    assert_eq!(fill.backfill_stage(), None);

    let row = stored(&repo, "deadbeef").await;
    assert_eq!(row.fee, Some(1234));
    assert!(row.is_complete());
}

#[tokio::test]
async fn backfill_fee_leaves_a_row_incomplete_while_its_parents_are_missing() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    repo.insert(&TxFixture::new("deadbeef").build())
        .await
        .expect("insert hollow tx");

    let fill = repo
        .backfill_fee("deadbeef", 1234)
        .await
        .expect("backfill fee")
        .expect("the row lacking a fee was filled");
    assert_eq!(fill.backfill_stage(), Some(BackfillStage::Raw));

    let row = stored(&repo, "deadbeef").await;
    assert_eq!(row.fee, Some(1234));
    assert!(row.input_txids.is_none(), "parents are still missing");
    assert!(!row.is_complete());
}

#[tokio::test]
async fn backfill_fee_never_overwrites_a_stored_fee() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    repo.insert(&TxFixture::new("deadbeef").with_fee(Some(500)).build())
        .await
        .expect("insert row with a fee");

    let fill = repo
        .backfill_fee("deadbeef", 999)
        .await
        .expect("backfill fee");
    assert_eq!(fill, None);
    assert_eq!(stored(&repo, "deadbeef").await.fee, Some(500));
}

#[tokio::test]
async fn insert_many_fills_only_the_missing_fee_of_a_row_already_stored() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool);

    for seed in [
        TxFixture::new("hollow").build(),
        TxFixture::new("no_fee")
            .with_vsize(TX_VSIZE)
            .with_input_txids(&["parent"])
            .build(),
        TxFixture::new("has_fee")
            .with_fee(Some(500))
            .with_vsize(TX_VSIZE)
            .build(),
    ] {
        repo.insert(&seed).await.expect("seed row");
    }

    // what bootstrap sends: a verbose entry per live txid
    let entry = |txid: &str, fee: u64| {
        NewTransaction::from(
            &MempoolEntryFixture::new(txid)
                .with_fee_in_sats(fee)
                .with_vsize(250)
                .build(),
        )
    };
    let written = repo
        .insert_many(&[
            entry("hollow", 10),
            entry("no_fee", 20),
            entry("has_fee", 30),
            entry("new", 40),
        ])
        .await
        .expect("insert many");
    assert_eq!(written, 3, "two fees filled, one row inserted");

    let hollow = stored(&repo, "hollow").await;
    assert_eq!(hollow.fee, Some(10));
    assert_eq!(hollow.vsize, 0, "only the fee is filled");
    assert!(!hollow.is_complete(), "parents and vsize are still missing");

    let no_fee = stored(&repo, "no_fee").await;
    assert_eq!(no_fee.fee, Some(20));
    assert_eq!(no_fee.vsize, TX_VSIZE);
    assert!(no_fee.is_complete());

    let has_fee = stored(&repo, "has_fee").await;
    assert_eq!(has_fee.fee, Some(500));
    assert_eq!(has_fee.vsize, TX_VSIZE);

    let new = stored(&repo, "new").await;
    assert_eq!((new.fee, new.vsize), (Some(40), 250));
}

#[tokio::test]
async fn insert_many_rolls_back_earlier_chunks_when_a_later_one_fails() {
    let pool = testkit::postgres::autocommit_pool().await;
    let repo = TransactionRepository::new(pool.clone());

    let batch_size = TRANSACTION_INSERT_CHUNK_SIZE + 1;
    // zero-padded so txid order matches insertion order: insert_many sorts by
    // txid, and this test needs the invalid row to land in the second chunk.
    let mut txs: Vec<NewTransaction> = (0..batch_size)
        .map(|i| TxFixture::new(&format!("atomicity-chunked-{i:04}")).build())
        .collect();
    // no `blocks` row exists for this hash, so the FK on `confirmed_at_block`
    // rejects this row -- the only row in the second chunk.
    txs.last_mut().unwrap().confirmed_at_block = Some("missing-block".to_string());

    repo.insert_many(&txs)
        .await
        .expect_err("confirmed_at_block with no matching blocks row violates the FK");

    let mut conn = pool.get().await.expect("conn");
    let count: i64 = transactions::table
        .filter(transactions::txid.like("atomicity-chunked-%"))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count transactions");

    // clean up before asserting, so a failed assertion does not leave real,
    // committed rows behind for the next test to run against this slot.
    diesel::delete(transactions::table.filter(transactions::txid.like("atomicity-chunked-%")))
        .execute(&mut conn)
        .await
        .expect("clean up inserted rows");

    assert_eq!(
        count, 0,
        "a failure in the second chunk must roll back the first chunk too"
    );
}
