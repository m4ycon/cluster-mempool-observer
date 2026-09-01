#![cfg(feature = "db_integration_tests")]

//! Regression suite for the split between `clusters.txids` (authoritative,
//! written from the node's `getmempoolcluster` answer) and
//! `transactions.cluster_id` (a back-link every write-path decision reads
//! instead of the cluster row). The two can silently disagree:
//! `UPDATE transactions SET cluster_id = ... WHERE txid = ANY(members)`
//! matches zero rows in silence when a member has no `transactions` row yet,
//! which happens routinely for ancestors/descendants the mempool poll has
//! not announced. Each repro is `#[ignore]`d with the step expected to fix
//! it; one guard-rail test asserts behavior that already holds and must
//! keep passing.

use api::db::models::NewTransaction;
use api::db::{Repos, TransactionRepository};
use std::collections::HashMap;
use testkit::deps::{cluster_service, deps};
use testkit::fixtures::{
    ClusterFixture, MempoolDeltaEventFixture, MempoolDeltaFixture, MempoolEntryFixture, TX_FEE,
    TX_VSIZE, TxFixture, fixed_time, seed_txs,
};
use testkit::mocks::MockClusterRetriever;
use testkit::postgres::isolated_pool;

#[tokio::test]
#[ignore = "repro: member txid with no transactions row never gets the cluster back-link; unignored by step 4a"]
async fn cluster_members_without_a_transactions_row_are_still_linked() {
    let pool = isolated_pool().await;
    let retriever =
        MockClusterRetriever::with_clusters(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool).with_cluster_retriever(retriever);

    // no seed_txs: a and b have no transactions row, exactly like ancestors
    // the 10-second getrawmempool poll has not announced yet
    deps.cluster_service()
        .sync_clusters_for(&["a".into(), "b".into()], &[])
        .await;

    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");
    let mut txids = stored.txids.clone();
    txids.sort();
    assert_eq!(
        txids,
        vec!["a".to_string(), "b".to_string()],
        "cluster row must hold both members regardless of whether a transactions row backs them"
    );

    let ids = deps
        .repos
        .transaction
        .get_cluster_ids_by_txids(&["a".into(), "b".into()])
        .await
        .expect("ids");
    assert_eq!(
        ids,
        vec![stored.id],
        "UPDATE transactions SET cluster_id = ... WHERE txid = ANY(members) matched zero rows, \
         so a and b never back-linked to the cluster that was just created for them"
    );
}

#[tokio::test]
async fn repeated_sync_of_the_same_group_never_creates_a_second_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let svc = cluster_service(pool, vec![ClusterFixture::new(&["a", "b"]).build()]);

    // three identical rounds, like three consecutive 10-second poll ticks
    // that keep reporting the same node-side group -- the "24 generations of
    // one cluster" incident
    for _ in 0..3 {
        svc.sync_clusters_for(&["a".into(), "b".into()], &[]).await;
    }

    assert_eq!(
        repos.cluster.count().await.expect("count"),
        1,
        "identical resyncs of the same group must update the existing cluster row, not insert a new one each time"
    );
}

#[tokio::test]
async fn a_block_mining_every_member_confirms_the_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let svc = cluster_service(pool, vec![ClusterFixture::new(&["a", "b"]).build()]);

    svc.sync_clusters_for(&["a".into(), "b".into()], &[]).await;
    let stored = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");

    let fees = HashMap::from([("a".to_string(), TX_FEE), ("b".to_string(), TX_FEE)]);
    let sizes = HashMap::from([("a".to_string(), TX_VSIZE), ("b".to_string(), TX_VSIZE)]);
    svc.confirm_mined(&["a".into(), "b".into()], &fees, &sizes, fixed_time())
        .await;

    assert!(
        repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "a cluster whose every member was just mined must leave the active set"
    );

    let confirmed = repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert!(
        confirmed.confirmed_at.is_some(),
        "a fully-mined cluster must carry confirmed_at, or the UI keeps showing a confirmed tx as pending"
    );
}

#[tokio::test]
async fn evicting_every_member_closes_the_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let svc = cluster_service(pool, vec![ClusterFixture::new(&["a", "b"]).build()]);

    svc.sync_clusters_for(&["a".into(), "b".into()], &[]).await;
    let stored = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");

    svc.sync_clusters_for(&[], &["a".into(), "b".into()]).await;

    let closed = repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert!(
        closed.txids.is_empty(),
        "a cluster whose every member was evicted must close (empty txids)"
    );
    assert!(
        repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "a closed cluster must not remain in the active set"
    );
}

#[tokio::test]
#[ignore = "repro: insert_many's on_conflict(txid).do_nothing() drops a later, fee/vsize-carrying insert onto an existing hollow row; unignored by step 4a"]
async fn a_hollow_row_never_shadows_a_later_insert_that_carries_fee_and_vsize() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool);

    // "a" is first seen only as a bare txid (an ancestor pulled in by a
    // cluster answer, say), so it lands hollow
    tx_repo
        .insert(&TxFixture::new("a").build())
        .await
        .expect("seed hollow row");

    // this is exactly the row bootstrap builds via NewTransaction::from(&MempoolEntrySummary)
    let entry = MempoolEntryFixture::new("a").build();
    tx_repo
        .insert_many(&[NewTransaction::from(&entry)])
        .await
        .expect("insert enriched row");

    let stored = tx_repo
        .find_by_txids(&["a".into()])
        .await
        .expect("query")
        .pop()
        .expect("row exists");
    assert_eq!(
        stored.fee,
        Some(TX_FEE),
        "on_conflict(txid).do_nothing() dropped the fee the later, better-informed insert carried"
    );
    assert_eq!(
        stored.vsize, TX_VSIZE,
        "on_conflict(txid).do_nothing() dropped the vsize the later, better-informed insert carried"
    );
    assert!(
        !stored.hollow,
        "the row stayed hollow forever: insert_many only ever no-ops on a conflicting txid, never updates it"
    );
}

#[tokio::test]
async fn a_cluster_whose_members_vanished_during_downtime_is_closed_by_reconciliation() {
    let pool = isolated_pool().await;
    let retriever =
        MockClusterRetriever::with_clusters(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool).with_cluster_retriever(retriever);

    // a and b entered the mempool before the outage, so they have an
    // unpaired add_mempool row; no transactions rows, because they were
    // never inserted before the api went down
    deps.repos
        .mempool_delta
        .insert_many(&[
            MempoolDeltaFixture::added("a").build(),
            MempoolDeltaFixture::added("b").build(),
        ])
        .await
        .expect("seed add deltas");

    deps.cluster_service()
        .sync_clusters_for(&["a".into(), "b".into()], &[])
        .await;

    // the node no longer reports a or b: this is what
    // BootstrapService::setup_mempool_snapshot drives on restart
    deps.mempool_service()
        .apply_bootstrap_delta(
            MempoolDeltaEventFixture::new()
                .with_removed(&["a", "b"])
                .build(),
            HashMap::new(),
        )
        .await;

    assert!(
        deps.repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "a cluster whose members left the mempool during downtime must close on bootstrap reconciliation"
    );
}

#[tokio::test]
#[ignore = "repro: record_removes_for_unpaired skips a candidate with no unpaired add_mempool row, so a member only ever seen through a cluster poll blocks the whole cluster from ever closing; unignored by step 4b"]
async fn a_member_only_ever_seen_through_a_cluster_poll_can_still_leave_the_mempool() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["a", "c"]).build()]);
    let deps = deps(pool).with_cluster_retriever(retriever.clone());

    // a's lineage is complete (transactions row + add_mempool event); c is
    // pulled in only as a's cluster-mate, with no add_mempool event of its own
    seed_txs(&deps.repos.transaction, &["a"]).await;
    deps.repos
        .mempool_delta
        .insert_many(&[MempoolDeltaFixture::added("a").build()])
        .await
        .expect("seed add delta");

    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;

    // the node now reports the whole group gone
    retriever.set_clusters(vec![]);

    deps.mempool_service()
        .apply_bootstrap_delta(
            MempoolDeltaEventFixture::new()
                .with_removed(&["a", "c"])
                .build(),
            HashMap::new(),
        )
        .await;

    assert!(
        deps.repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "c has no unpaired add_mempool row, so record_removes_for_unpaired skips it: only a is \
         recorded as evicted, the cluster keeps its one remaining (phantom) member, and it never closes"
    );
}

#[tokio::test]
async fn a_hollow_insert_never_downgrades_a_row_that_already_carries_fee_and_vsize() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool);

    tx_repo
        .insert(&TxFixture::new("a").sized().build())
        .await
        .expect("seed sized row");

    // guard rail for step 4a's insert_many upsert, not a repro: this already
    // passes, because on_conflict(txid).do_nothing() leaves the row alone
    tx_repo
        .insert_many(&[NewTransaction::hollow("a")])
        .await
        .expect("insert hollow row");

    let stored = tx_repo
        .find_by_txids(&["a".into()])
        .await
        .expect("query")
        .pop()
        .expect("row exists");
    assert_eq!(
        stored.fee,
        Some(TX_FEE),
        "a later hollow insert must never erase a fee the row already carried"
    );
    assert_eq!(
        stored.vsize, TX_VSIZE,
        "a later hollow insert must never erase a vsize the row already carried"
    );
    assert!(
        !stored.hollow,
        "a later hollow insert must never flip an already-enriched row back to hollow"
    );
}
