#![cfg(all(feature = "db_integration_tests", feature = "node_integration_tests"))]

use api::db::models::NewMempoolDelta;
use api::db::{MempoolDeltaRepository, TransactionRepository};
use api::infra::state::AppState;
use observer::infra::config::Config as ObserverConfig;
use std::collections::HashSet;
use std::slice;
use testkit::node::{maturate_coinbase, rpc_config, send_to_address, setup_node};
use testkit::postgres::isolated_pool;

#[tokio::test]
async fn bootstrap_on_empty_db_records_live_mempool_and_seeds_snapshot() {
    let node = setup_node();
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let txid = send_to_address(&node, &address).to_string();

    let pool = isolated_pool().await;
    let mempool_delta_repo = MempoolDeltaRepository::new(pool.clone());
    let transaction_repo = TransactionRepository::new(pool.clone());
    let config = ObserverConfig {
        rpc: rpc_config(&node),
        ..Default::default()
    };
    let (state, snapshot, _clients) = AppState::build(&config, pool);

    state
        .bootstrap_service
        .setup_mempool_snapshot(&snapshot)
        .await;

    let live = HashSet::from([txid.clone()]);
    assert_eq!(snapshot.get(), live, "snapshot seeded with live mempool");

    assert_eq!(
        mempool_delta_repo.count().await.unwrap(),
        1,
        "one reconciliation delta"
    );
    assert_eq!(
        mempool_delta_repo.reconstruct_snapshot().await.unwrap(),
        live,
        "log reconstructs to the live set"
    );
    assert_eq!(
        transaction_repo
            .existing_txids(slice::from_ref(&txid))
            .await
            .unwrap(),
        vec![txid],
        "live tx backfilled"
    );
}

#[tokio::test]
async fn bootstrap_records_only_the_diff_between_past_and_live_state() {
    let node = setup_node();
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let tx1 = send_to_address(&node, &address).to_string();
    let tx2 = send_to_address(&node, &address).to_string();

    let pool = isolated_pool().await;
    let mempool_delta_repo = MempoolDeltaRepository::new(pool.clone());
    let config = ObserverConfig {
        rpc: rpc_config(&node),
        ..Default::default()
    };
    let (state, snapshot, _clients) = AppState::build(&config, pool);

    // tx1 is already known, tx2 is new, and a stale tx is removed
    let stale = "0".repeat(64);
    mempool_delta_repo
        .insert(&NewMempoolDelta {
            added: vec![tx1.clone(), stale.clone()],
            removed: vec![],
        })
        .await
        .expect("seed past delta");

    state
        .bootstrap_service
        .setup_mempool_snapshot(&snapshot)
        .await;

    let live = HashSet::from([tx1.clone(), tx2.clone()]);
    assert_eq!(snapshot.get(), live, "snapshot reflects live mempool");

    assert_eq!(
        mempool_delta_repo.reconstruct_snapshot().await.unwrap(),
        live,
        "log reconstructs to live: tx2 added, stale removed, tx1 untouched"
    );
    assert_eq!(
        mempool_delta_repo.count().await.unwrap(),
        2,
        "one reconciliation delta on top of the seeded one"
    );
}

#[tokio::test]
async fn bootstrap_writes_no_delta_when_past_state_matches_live() {
    let node = setup_node();
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let txid = send_to_address(&node, &address).to_string();

    let pool = isolated_pool().await;
    let mempool_delta_repo = MempoolDeltaRepository::new(pool.clone());
    let config = ObserverConfig {
        rpc: rpc_config(&node),
        ..Default::default()
    };
    let (state, snapshot, _clients) = AppState::build(&config, pool);

    // Past state already matches the live mempool
    mempool_delta_repo
        .insert(&NewMempoolDelta {
            added: vec![txid.clone()],
            removed: vec![],
        })
        .await
        .expect("seed past delta");

    state
        .bootstrap_service
        .setup_mempool_snapshot(&snapshot)
        .await;

    assert_eq!(
        mempool_delta_repo.count().await.unwrap(),
        1,
        "no reconciliation row when nothing changed"
    );
    assert_eq!(
        snapshot.get(),
        HashSet::from([txid]),
        "snapshot still seeded"
    );
}
