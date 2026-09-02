//! `MempoolAdmissionRepository` integration tests.
#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, NewTransaction};
use api::db::schema::{mempool_deltas, transactions};
use api::db::{MempoolAdmissionRepository, TRANSACTION_INSERT_CHUNK_SIZE};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use testkit::fixtures::TxFixture;
use testkit::postgres::isolated_pool;

#[tokio::test]
async fn admit_writes_an_add_event_for_every_added_txid() {
    let pool = isolated_pool().await;
    let repo = MempoolAdmissionRepository::new(pool.clone());

    repo.admit(
        &["known".to_string(), "fresh".to_string()],
        &[TxFixture::new("fresh").build()],
    )
    .await
    .expect("admit");

    let mut conn = pool.get().await.expect("conn");
    let mut events: Vec<(String, DeltaReason)> = mempool_deltas::table
        .select((mempool_deltas::txid, mempool_deltas::reason))
        .load(&mut conn)
        .await
        .expect("load deltas");
    events.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(
        events,
        vec![
            ("fresh".to_string(), DeltaReason::AddMempool),
            ("known".to_string(), DeltaReason::AddMempool),
        ],
        "an add_mempool event is recorded for every added txid, not just the new ones"
    );
}

#[tokio::test]
async fn admit_only_inserts_rows_for_the_txids_passed_as_new() {
    let pool = isolated_pool().await;
    let repo = MempoolAdmissionRepository::new(pool.clone());

    // "known" already has a row; it is still in `added` (an event must be
    // recorded for it) but must not be passed in `new_txs`.
    repo.admit(
        &["known".to_string(), "fresh".to_string()],
        &[TxFixture::new("fresh").build()],
    )
    .await
    .expect("admit");

    let mut conn = pool.get().await.expect("conn");
    let txids: Vec<String> = transactions::table
        .select(transactions::txid)
        .load(&mut conn)
        .await
        .expect("load transactions");

    assert_eq!(txids, vec!["fresh".to_string()]);
}

#[tokio::test]
async fn admit_on_an_empty_added_list_writes_nothing() {
    let pool = isolated_pool().await;
    let repo = MempoolAdmissionRepository::new(pool.clone());

    repo.admit(&[], &[]).await.expect("admit empty");

    let mut conn = pool.get().await.expect("conn");
    let count: i64 = mempool_deltas::table
        .count()
        .get_result(&mut conn)
        .await
        .expect("count deltas");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn admit_rolls_back_the_add_events_when_the_transactions_insert_fails() {
    let pool = testkit::postgres::autocommit_pool().await;
    let repo = MempoolAdmissionRepository::new(pool.clone());

    let batch_size = TRANSACTION_INSERT_CHUNK_SIZE + 1;
    let added: Vec<String> = (0..batch_size)
        .map(|i| format!("admission-atomicity-{i}"))
        .collect();
    let mut new_txs: Vec<NewTransaction> = added
        .iter()
        .map(|txid| TxFixture::new(txid).build())
        .collect();
    // no `blocks` row exists for this hash, so the FK on `confirmed_at_block`
    // rejects this row -- the only row in the second transactions chunk.
    new_txs.last_mut().unwrap().confirmed_at_block = Some("missing-block".to_string());

    repo.admit(&added, &new_txs)
        .await
        .expect_err("confirmed_at_block with no matching blocks row violates the FK");

    let mut conn = pool.get().await.expect("conn");
    let delta_count: i64 = mempool_deltas::table
        .filter(mempool_deltas::txid.like("admission-atomicity-%"))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count deltas");
    let tx_count: i64 = transactions::table
        .filter(transactions::txid.like("admission-atomicity-%"))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count transactions");

    // clean up before asserting, so a failed assertion does not leave real,
    // committed rows behind for the next test to run against this slot.
    diesel::delete(
        mempool_deltas::table.filter(mempool_deltas::txid.like("admission-atomicity-%")),
    )
    .execute(&mut conn)
    .await
    .expect("clean up delta rows");
    diesel::delete(transactions::table.filter(transactions::txid.like("admission-atomicity-%")))
        .execute(&mut conn)
        .await
        .expect("clean up transaction rows");

    assert_eq!(
        delta_count, 0,
        "a failed transactions insert must roll back the add_mempool events beside it"
    );
    assert_eq!(
        tx_count, 0,
        "a failure in the second chunk must roll back the first chunk too"
    );
}
