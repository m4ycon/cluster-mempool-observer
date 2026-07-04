//! `MempoolDeltaRepository` integration tests (snapshot reconstruction).
#![cfg(feature = "db_integration_tests")]

use api::db::MempoolDeltaRepository;
use api::db::models::{DeltaReason, NewMempoolDelta};
use testkit::postgres::isolated_pool;

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
