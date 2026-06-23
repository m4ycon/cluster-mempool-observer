//! `MempoolDeltaRepository` integration tests (snapshot reconstruction).
#![cfg(feature = "db_integration_tests")]

use api::db::MempoolDeltaRepository;
use api::db::models::NewMempoolDelta;
use testkit::postgres::isolated_pool;

#[tokio::test]
async fn reconstruct_snapshot_folds_added_and_removed_in_order() {
    let pool = isolated_pool().await;
    let mempool_repo = MempoolDeltaRepository::new(pool);

    // +{a, b}, -{b} and +{c} => {a, c}
    mempool_repo
        .insert(&NewMempoolDelta {
            added: vec!["a".into(), "b".into()],
            removed: vec![],
        })
        .await
        .expect("insert delta 1");
    mempool_repo
        .insert(&NewMempoolDelta {
            added: vec!["c".into()],
            removed: vec!["b".into()],
        })
        .await
        .expect("insert delta 2");

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
async fn reconstruct_snapshot_is_empty_with_no_history() {
    let pool = isolated_pool().await;
    let mempool_repo = MempoolDeltaRepository::new(pool);

    let set = mempool_repo
        .reconstruct_snapshot()
        .await
        .expect("reconstruct");

    assert!(set.is_empty());
}
