#![cfg(all(feature = "db_integration_tests", feature = "node_integration_tests"))]

use api::db::models::SystemEventKind;
use api::db::schema::{blocks, transactions};
use api::db::{BlockRepository, DbPool, MempoolDeltaRepository, Repos, SystemEventRepository};
use api::infra::config::ApiConfig;
use api::infra::deps::Deps;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use observer::infra::config::{Config as ObserverConfig, ZmqConfig};
use std::collections::HashSet;
use std::slice;
use testkit::config::INERT_ZMQ_ENDPOINT;
use testkit::deps::clients_for_node;
use testkit::fixtures::{MempoolDeltaFixture, NewBlockFixture};
use testkit::metrics::{assert_series, capture};
use testkit::node::{Node, maturate_coinbase, rpc_config, send_to_address, setup_node};
use testkit::postgres::isolated_pool;

/// Runs the full bootstrap flow against the live node, returning the ledger's
/// live set (post-reconciliation) so tests can assert on it.
async fn run_bootstrap(node: &Node, pool: DbPool) -> HashSet<String> {
    run_bootstrap_with_deps(node, pool)
        .await
        .mempool_ledger
        .clone_live_snapshot()
}

/// Same run, handing back the whole `Deps` for tests that assert on state the
/// ledger does not carry, such as the backfill queue.
async fn run_bootstrap_with_deps(node: &Node, pool: DbPool) -> Deps {
    let cfg = ApiConfig {
        observer: ObserverConfig {
            rpc: rpc_config(node),
            zmq: ZmqConfig {
                blocks_endpoint: INERT_ZMQ_ENDPOINT.into(),
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let clients = clients_for_node(node);
    let deps = Deps::new(Repos::new(pool), &clients);
    let feerate_diagram_snapshot = deps.feerate_diagram_snapshot.clone();
    let state = deps.app_state();
    state
        .bootstrap_service
        .run(&cfg, clients, feerate_diagram_snapshot)
        .await;

    // Bootstrap only queues the reconciliation into the ledger's journal now;
    // the reconciler is the one that actually writes mempool_deltas, so tests
    // asserting on that table need to flush it themselves.
    deps.mempool_reconciler().tick().await;

    deps
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
        .insert(&NewBlockFixture::new("seed-block-101", 101).build())
        .await
        .expect("seed block 101");

    // chain advances to 103 while we were "down"
    node.client
        .generate_to_address(2, &address)
        .expect("mine 2 blocks");

    run_bootstrap(&node, pool.clone()).await;

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

    run_bootstrap(&node, pool.clone()).await;

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
    let repos = Repos::new(pool.clone());

    let live_txids = run_bootstrap(&node, pool.clone()).await;

    let live = HashSet::from([txid.clone()]);
    assert_eq!(live_txids, live, "ledger seeded with live mempool");

    assert_eq!(
        repos.mempool_delta.count().await.unwrap(),
        1,
        "one reconciliation delta"
    );
    assert_eq!(
        repos.mempool_delta.reconstruct_snapshot().await.unwrap(),
        live,
        "log reconstructs to the live set"
    );
    assert_eq!(
        repos
            .transaction
            .existing_txids(slice::from_ref(&txid))
            .await
            .unwrap(),
        vec![txid.clone()],
        "live tx backfilled"
    );

    // built from the verbose entry, not from a per-tx fetch
    let mut conn = pool.get().await.expect("conn");
    let (fee, vsize, hollow): (Option<i64>, i64, bool) = transactions::table
        .filter(transactions::txid.eq(&txid))
        .select((transactions::fee, transactions::vsize, transactions::hollow))
        .first(&mut conn)
        .await
        .expect("load backfilled tx");
    assert!(fee.is_some_and(|f| f > 0), "fee came from the entry");
    assert!(vsize > 0, "vsize came from the entry");
    assert!(!hollow, "nothing hollow on a healthy bootstrap");
}

#[tokio::test]
async fn bootstrap_queues_the_rows_it_wrote_for_parent_backfill() {
    let node = setup_node();
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let txid = send_to_address(&node, &address).to_string();

    let pool = isolated_pool().await;
    let deps = run_bootstrap_with_deps(&node, pool.clone()).await;

    // A verbose entry has no vin, so the row bootstrap just wrote carries no
    // parents. Nothing else will queue it: the reconciler enqueues only adds
    // that had no row, and this one has had a row since before the tick.
    let mut conn = pool.get().await.expect("conn");
    let input_txids: Option<Vec<String>> = transactions::table
        .filter(transactions::txid.eq(&txid))
        .select(transactions::input_txids)
        .first(&mut conn)
        .await
        .expect("load bootstrapped tx");
    assert!(
        input_txids.is_none(),
        "the verbose entry cannot supply parents, so they must still be missing"
    );

    let mut queued = Vec::new();
    let mut rx = deps
        .tx_backfill_queue
        .take_receiver()
        .expect("receiver not taken yet");
    while let Ok(req) = rx.try_recv() {
        queued.push(req.txid);
    }
    assert_eq!(
        queued,
        vec![txid],
        "a mempool tx present at boot must be queued for getrawtransaction, or it keeps \
         its NULL input_txids forever and never enters the dependency graph"
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
            MempoolDeltaFixture::added(&tx1).build(),
            MempoolDeltaFixture::added(&stale).build(),
        ])
        .await
        .expect("seed past delta");

    let live_txids = run_bootstrap(&node, pool.clone()).await;

    let live = HashSet::from([tx1.clone(), tx2.clone()]);
    assert_eq!(live_txids, live, "ledger reflects live mempool");

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
async fn bootstrap_records_started_then_completed_with_reconciliation_counts() {
    let node = setup_node();
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let tx1 = send_to_address(&node, &address).to_string();
    let _tx2 = send_to_address(&node, &address).to_string();

    let pool = isolated_pool().await;
    let mempool_delta_repo = MempoolDeltaRepository::new(pool.clone());

    // tx1 is already known, tx2 is new, and a stale tx is removed
    let stale = "0".repeat(64);
    mempool_delta_repo
        .insert_many(&[
            MempoolDeltaFixture::added(&tx1).build(),
            MempoolDeltaFixture::added(&stale).build(),
        ])
        .await
        .expect("seed past delta");

    run_bootstrap(&node, pool.clone()).await;

    let system_event_repo = SystemEventRepository::new(pool);
    let events = system_event_repo
        .list(None, None)
        .await
        .expect("list system events");

    let started_idx = events
        .iter()
        .position(|e| e.kind == SystemEventKind::BootstrapStarted)
        .expect("bootstrap_started recorded");
    let completed_idx = events
        .iter()
        .position(|e| e.kind == SystemEventKind::BootstrapCompleted)
        .expect("bootstrap_completed recorded");
    assert!(
        started_idx < completed_idx,
        "bootstrap_started must precede bootstrap_completed"
    );

    let details = &events[completed_idx].details;
    assert_eq!(details["txs_added"], 1, "tx2 is the only new txid");
    assert_eq!(
        details["txs_removed"], 1,
        "the stale txid is reconciled away"
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
        .insert_many(&[MempoolDeltaFixture::added(&txid).build()])
        .await
        .expect("seed past delta");

    let live_txids = run_bootstrap(&node, pool.clone()).await;

    assert_eq!(
        mempool_delta_repo.count().await.unwrap(),
        1,
        "no reconciliation row when nothing changed"
    );
    assert_eq!(live_txids, HashSet::from([txid]), "ledger still seeded");
}

#[test]
fn bootstrap_stage_seconds_records_every_startup_stage() {
    let node = setup_node();
    let rendered = capture(async {
        run_bootstrap(&node, isolated_pool().await).await;
    });

    for stage in ["sync_missing_blocks", "mempool_snapshot", "seed_clusters"] {
        assert_series(
            &rendered,
            &format!(r#"bootstrap_stage_seconds_count{{stage="{stage}"}} 1"#),
        );
    }
}
