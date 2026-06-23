#![cfg(all(feature = "db_integration_tests", feature = "node_integration_tests"))]

use api::db::models::NewMempoolDelta;
use api::infra::state::AppState;
use api::services::bootstrap::bootstrap;
use observer::clients::Clients;
use shared::pubsub::PubSub;
use std::collections::HashSet;
use std::slice;
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};
use testkit::postgres::isolated_pool;

#[tokio::test]
async fn bootstrap_on_empty_db_records_live_mempool_and_seeds_snapshot() {
    let (node, rpc) = setup_node_and_rpc_client();
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let txid = send_to_address(&node, &address).to_string();

    let pool = isolated_pool().await;
    let (state, snapshot) = AppState::build(
        Clients {
            pubsub: PubSub::new(),
            rpc,
        },
        pool,
    );

    bootstrap(&state, &snapshot).await;

    let live = HashSet::from([txid.clone()]);
    assert_eq!(snapshot.get(), live, "snapshot seeded with live mempool");

    let deltas = &state.mempool_delta_repository;
    assert_eq!(deltas.count().await.unwrap(), 1, "one reconciliation delta");
    assert_eq!(
        deltas.reconstruct_snapshot().await.unwrap(),
        live,
        "log reconstructs to the live set"
    );
    assert_eq!(
        state
            .transaction_repository
            .existing_txids(slice::from_ref(&txid))
            .await
            .unwrap(),
        vec![txid],
        "live tx backfilled"
    );
}

#[tokio::test]
async fn bootstrap_records_only_the_diff_between_past_and_live_state() {
    let (node, rpc) = setup_node_and_rpc_client();
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let tx1 = send_to_address(&node, &address).to_string();
    let tx2 = send_to_address(&node, &address).to_string();

    let pool = isolated_pool().await;
    let (state, snapshot) = AppState::build(
        Clients {
            pubsub: PubSub::new(),
            rpc,
        },
        pool,
    );

    // tx1 is already known, tx2 is new, and a stale tx is removed
    let stale = "0".repeat(64);
    state
        .mempool_delta_repository
        .insert(&NewMempoolDelta {
            added: vec![tx1.clone(), stale.clone()],
            removed: vec![],
        })
        .await
        .expect("seed past delta");

    bootstrap(&state, &snapshot).await;

    let live = HashSet::from([tx1.clone(), tx2.clone()]);
    assert_eq!(snapshot.get(), live, "snapshot reflects live mempool");

    let deltas = &state.mempool_delta_repository;
    assert_eq!(
        deltas.reconstruct_snapshot().await.unwrap(),
        live,
        "log reconstructs to live: tx2 added, stale removed, tx1 untouched"
    );
    assert_eq!(
        deltas.count().await.unwrap(),
        2,
        "one reconciliation delta on top of the seeded one"
    );
}

#[tokio::test]
async fn bootstrap_writes_no_delta_when_past_state_matches_live() {
    let (node, rpc) = setup_node_and_rpc_client();
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let txid = send_to_address(&node, &address).to_string();

    let pool = isolated_pool().await;
    let (state, snapshot) = AppState::build(
        Clients {
            pubsub: PubSub::new(),
            rpc,
        },
        pool,
    );

    // Past state already matches the live mempool
    state
        .mempool_delta_repository
        .insert(&NewMempoolDelta {
            added: vec![txid.clone()],
            removed: vec![],
        })
        .await
        .expect("seed past delta");

    bootstrap(&state, &snapshot).await;

    assert_eq!(
        state.mempool_delta_repository.count().await.unwrap(),
        1,
        "no reconciliation row when nothing changed"
    );
    assert_eq!(
        snapshot.get(),
        HashSet::from([txid]),
        "snapshot still seeded"
    );
}
