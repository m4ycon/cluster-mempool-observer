#![cfg(all(feature = "db_integration_tests", feature = "node_integration_tests"))]

use api::db::models::{DeltaReason, NewBlock, NewMempoolDelta};
use api::db::schema::blocks;
use api::db::{BlockRepository, MempoolDeltaRepository, TransactionRepository};
use api::infra::config::ApiConfig;
use api::infra::state::AppState;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use observer::infra::config::{Config as ObserverConfig, RpcConfig};
use observer::snapshot::MempoolSnapshot;
use std::collections::HashSet;
use std::slice;
use testkit::node::{maturate_coinbase, rpc_config, send_to_address, setup_node};
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

/// Runs the full bootstrap flow against the live node, returning the seeded
/// mempool snapshot so tests can assert on the reconciled state.
async fn run_bootstrap(rpc: RpcConfig, pool: api::db::DbPool) -> MempoolSnapshot {
    let cfg = ApiConfig {
        observer: ObserverConfig {
            rpc,
            ..Default::default()
        },
        ..Default::default()
    };
    let (state, snapshot, clients) = AppState::build(&cfg.observer, pool);
    let result = snapshot.clone();
    state.bootstrap_service.run(&cfg, clients, snapshot).await;
    result
}

#[tokio::test]
async fn bootstrap_backfills_blocks_missed_while_down() {
    let pool = isolated_pool().await;
    let node = setup_node();

    // tip 101 after maturity
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);

    // pretend the DB already processed up to height 101
    BlockRepository::new(pool.clone())
        .insert(&NewBlock {
            hash: "seed-block-101".to_string(),
            height: 101,
            mined_at: OffsetDateTime::now_utc(),
            tx_count: 0,
            total_bytes: 0,
            total_fee: 0,
            difficulty: 0.0,
        })
        .await
        .expect("seed block 101");

    // chain advances to 103 while we were "down"
    node.client
        .generate_to_address(2, &address)
        .expect("mine 2 blocks");

    run_bootstrap(rpc_config(&node), pool.clone()).await;

    let mut conn = pool.get().await.expect("conn");

    let max_height: Option<i64> = blocks::table
        .select(diesel::dsl::max(blocks::height))
        .first(&mut conn)
        .await
        .expect("max height");
    assert_eq!(max_height, Some(103), "tip should be persisted");

    for height in [102_i64, 103_i64] {
        blocks::table
            .filter(blocks::height.eq(height))
            .select(blocks::hash)
            .first::<String>(&mut conn)
            .await
            .unwrap_or_else(|e| panic!("height {height} should be backfilled: {e}"));
    }
}

#[tokio::test]
async fn bootstrap_cold_start_syncs_tip_only() {
    let pool = isolated_pool().await;
    let node = setup_node();

    // tip 104, DB empty
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    node.client
        .generate_to_address(3, &address)
        .expect("mine 3 blocks");

    run_bootstrap(rpc_config(&node), pool.clone()).await;

    let mut conn = pool.get().await.expect("conn");

    let count: i64 = blocks::table
        .count()
        .get_result(&mut conn)
        .await
        .expect("count blocks");
    assert_eq!(count, 1, "cold start should persist only the tip block");

    let height: i64 = blocks::table
        .select(blocks::height)
        .first(&mut conn)
        .await
        .expect("the one block height");
    assert_eq!(height, 104, "the single persisted block is the tip");
}

#[tokio::test]
async fn bootstrap_on_empty_db_records_live_mempool_and_seeds_snapshot() {
    let node = setup_node();
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let txid = send_to_address(&node, &address).to_string();

    let pool = isolated_pool().await;
    let mempool_delta_repo = MempoolDeltaRepository::new(pool.clone());
    let transaction_repo = TransactionRepository::new(pool.clone());

    let snapshot = run_bootstrap(rpc_config(&node), pool.clone()).await;

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

    // tx1 is already known, tx2 is new, and a stale tx is removed
    let stale = "0".repeat(64);
    mempool_delta_repo
        .insert_many(&[
            NewMempoolDelta {
                txid: tx1.clone(),
                reason: DeltaReason::AddMempool,
            },
            NewMempoolDelta {
                txid: stale.clone(),
                reason: DeltaReason::AddMempool,
            },
        ])
        .await
        .expect("seed past delta");

    let snapshot = run_bootstrap(rpc_config(&node), pool.clone()).await;

    let live = HashSet::from([tx1.clone(), tx2.clone()]);
    assert_eq!(snapshot.get(), live, "snapshot reflects live mempool");

    assert_eq!(
        mempool_delta_repo.reconstruct_snapshot().await.unwrap(),
        live,
        "log reconstructs to live: tx2 added, stale removed, tx1 untouched"
    );
    assert_eq!(
        mempool_delta_repo.count().await.unwrap(),
        4,
        "two reconciliation rows (tx2 add_mempool, stale remove_evicted) on top of the two seeded"
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

    // Past state already matches the live mempool
    mempool_delta_repo
        .insert_many(&[NewMempoolDelta {
            txid: txid.clone(),
            reason: DeltaReason::AddMempool,
        }])
        .await
        .expect("seed past delta");

    let snapshot = run_bootstrap(rpc_config(&node), pool.clone()).await;

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
