//! `MempoolDeltaRepository` integration tests (snapshot reconstruction).
#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, NewMempoolDelta};
use api::db::schema::mempool_deltas;
use api::db::{DbPool, MempoolDeltaRepository};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use testkit::postgres::isolated_pool;
use time::{Duration, OffsetDateTime};

fn row(txid: &str, reason: DeltaReason) -> NewMempoolDelta {
    NewMempoolDelta {
        txid: txid.into(),
        reason,
    }
}

#[tokio::test]
async fn reconstruct_snapshot_folds_reasons_in_order() {
    let pool = isolated_pool().await;
    let mempool_repo = MempoolDeltaRepository::new(pool);

    // add a, add b, evict b, add c => {a, c}
    mempool_repo
        .insert_many(&[
            row("a", DeltaReason::AddMempool),
            row("b", DeltaReason::AddMempool),
        ])
        .await
        .expect("insert batch 1");
    mempool_repo
        .insert_many(&[
            row("b", DeltaReason::RemoveEvicted),
            row("c", DeltaReason::AddMempool),
        ])
        .await
        .expect("insert batch 2");

    let mut reconstructed: Vec<String> = mempool_repo
        .reconstruct_snapshot()
        .await
        .expect("reconstruct")
        .into_iter()
        .collect();
    reconstructed.sort();

    assert_eq!(reconstructed, vec!["a".to_string(), "c".to_string()]);
}

#[tokio::test]
async fn reconstruct_snapshot_drops_confirmed_txids() {
    let pool = isolated_pool().await;
    let mempool_repo = MempoolDeltaRepository::new(pool);

    // add a & b, then b is confirmed out of the mempool => {a}
    mempool_repo
        .insert_many(&[
            row("a", DeltaReason::AddMempool),
            row("b", DeltaReason::AddMempool),
            row("b", DeltaReason::RemoveConfirmed),
        ])
        .await
        .expect("insert batch");

    let reconstructed = mempool_repo
        .reconstruct_snapshot()
        .await
        .expect("reconstruct");

    assert_eq!(
        reconstructed,
        std::collections::HashSet::from(["a".to_string()])
    );
}

#[tokio::test]
async fn reconstruct_snapshot_is_empty_with_no_history() {
    let pool = isolated_pool().await;
    let mempool_repo = MempoolDeltaRepository::new(pool);

    let set = mempool_repo
        .reconstruct_snapshot()
        .await
        .expect("reconstruct");

    assert!(set.is_empty());
}

/// Overrides the DB-set `created_at` so the window boundary can be exercised.
async fn backdate(pool: &DbPool, txid: &str, at: OffsetDateTime) {
    let mut conn = pool.get().await.expect("conn");
    diesel::update(mempool_deltas::table.filter(mempool_deltas::txid.eq(txid)))
        .set(mempool_deltas::created_at.eq(at))
        .execute(&mut conn)
        .await
        .expect("backdate created_at");
}

#[tokio::test]
async fn count_adds_since_counts_only_in_window_adds() {
    let pool = isolated_pool().await;
    let repo = MempoolDeltaRepository::new(pool.clone());

    repo.insert_many(&[
        row("old_add", DeltaReason::AddMempool),
        row("recent_add", DeltaReason::AddMempool),
        row("recent_remove", DeltaReason::RemoveEvicted),
    ])
    .await
    .expect("insert deltas");

    let now = OffsetDateTime::now_utc();
    backdate(&pool, "old_add", now - Duration::seconds(90)).await;
    backdate(&pool, "recent_add", now - Duration::seconds(30)).await;
    backdate(&pool, "recent_remove", now - Duration::seconds(10)).await;

    // cutoff at 60s
    let count = repo
        .count_adds_since(now - Duration::seconds(60))
        .await
        .expect("count adds");
    assert_eq!(count, 1);
}
