#![cfg(feature = "db_integration_tests")]

//! Regression suite for the split between `clusters.txids` (authoritative,
//! written from the node's `getmempoolcluster` answer) and
//! `transactions.cluster_id` (a back-link every write-path decision reads
//! instead of the cluster row). The two can silently disagree:
//! `UPDATE transactions SET cluster_id = ... WHERE txid = ANY(members)`
//! matches zero rows in silence when a member has no `transactions` row yet,
//! which happens routinely for ancestors/descendants the mempool poll has
//! not announced.

use api::db::models::{ClusterStatus, DeltaReason, NewTransaction};
use api::db::schema::{cluster_deltas, mempool_deltas};
use api::db::{DbPool, Repos, TransactionRepository};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::events::BlockConnectedEvent;
use std::collections::{HashMap, HashSet};
use testkit::deps::{cluster_service, deps};
use testkit::fixtures::{
    BlockFixture, ClusterFixture, NewBlockFixture, TX_FEE, TX_VSIZE, TxFixture, fixed_time,
};
use testkit::mocks::{MockBlockRetriever, MockClusterRetriever};
use testkit::postgres::isolated_pool;
use time::{Duration, OffsetDateTime};

#[tokio::test]
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
    assert_eq!(
        closed.status,
        ClusterStatus::Evicted,
        "a cluster whose every member was evicted must be marked evicted"
    );
    assert!(
        !closed.txids.is_empty(),
        "closing must keep the cluster's membership, not empty it"
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
async fn a_cluster_whose_members_vanished_during_downtime_is_closed_by_reconciliation() {
    let pool = isolated_pool().await;
    let retriever =
        MockClusterRetriever::with_clusters(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool).with_cluster_retriever(retriever);

    // a and b entered the mempool before the outage, so the ledger already
    // considers them live; no transactions rows, because they were never
    // inserted before the api went down
    deps.mempool_ledger
        .seed(HashSet::from(["a".to_string(), "b".to_string()]));

    deps.cluster_service()
        .sync_clusters_for(&["a".into(), "b".into()], &[])
        .await;

    // the node no longer reports a or b: this is what
    // BootstrapService::setup_mempool_snapshot drives on restart, followed by
    // the reconciler's own tick
    deps.mempool_ledger.submit_authoritative(HashSet::new());
    deps.mempool_reconciler().tick().await;

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
async fn a_member_only_ever_seen_through_a_cluster_poll_can_still_leave_the_mempool() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["a", "c"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    // the node's getmempoolcluster answer for "a" pulls "c" along as its cluster-mate
    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;
    reconciler.tick().await;

    // the node now reports the whole group gone
    retriever.set_clusters(vec![]);

    // getrawmempool no longer reports a or c: submit_authoritative queues a
    // Remove for both, and the reconciler's flush closes the cluster through
    // handle_evicted
    deps.mempool_ledger.submit_authoritative(HashSet::new());
    reconciler.tick().await;

    assert!(
        deps.repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "c was only ever seen through the cluster poll, never reported by getrawmempool; it \
         must still be evicted when the watcher stops reporting the group, closing the cluster"
    );

    // "c" earns the same pair as "a" despite never being polled directly.
    let mut conn = pool.get().await.expect("checkout connection");
    for txid in ["a", "c"] {
        let reasons: Vec<DeltaReason> = mempool_deltas::table
            .filter(mempool_deltas::txid.eq(txid))
            .order(mempool_deltas::id.asc())
            .select(mempool_deltas::reason)
            .load(&mut conn)
            .await
            .expect("load delta reasons");
        assert_eq!(
            reasons,
            vec![DeltaReason::AddMempool, DeltaReason::RemoveEvicted],
            "{txid} must carry exactly one add/remove pair, both written by the reconciler"
        );
    }
}

#[tokio::test]
async fn a_cluster_whose_members_are_evicted_across_two_flushes_still_closes() {
    let pool = isolated_pool().await;
    let retriever =
        MockClusterRetriever::strict(vec![ClusterFixture::new(&["a", "b", "c", "d"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");

    // the node has already dropped a as well, but getrawmempool lags behind it:
    // only b, c, d are evicted by this flush, and the survivor lookup for a is
    // what queues a's Remove, so a is evicted by the next flush on its own
    retriever.set_clusters(vec![]);
    deps.mempool_ledger
        .submit_authoritative(HashSet::from(["a".to_string()]));
    reconciler.tick().await;
    assert_eq!(
        evicted_txids(&pool).await,
        vec!["b", "c", "d"],
        "the first flush must evict only b, c and d, or this is not the two-flush split"
    );

    deps.mempool_ledger.submit_authoritative(HashSet::new());
    reconciler.tick().await;
    assert_eq!(
        evicted_txids(&pool).await,
        vec!["a", "b", "c", "d"],
        "the second flush must evict a on its own"
    );

    let closed = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        closed.status,
        ClusterStatus::Evicted,
        "every member left the mempool, just not in the same flush; the cluster must \
         still be marked evicted instead of staying active forever"
    );
    assert!(
        deps.repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "a cluster with no member left in the mempool must not remain in the active set"
    );
}

#[tokio::test]
async fn a_cluster_with_one_member_evicted_and_the_other_mined_confirms_as_the_mined_member() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["x", "y"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["x".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("x")
        .await
        .expect("query")
        .expect("cluster exists");

    // x was mined, so the node no longer knows it, but getrawmempool still
    // reports it until the next poll
    retriever.set_clusters(vec![]);
    deps.mempool_ledger
        .submit_authoritative(HashSet::from(["x".to_string()]));
    reconciler.tick().await;
    assert_eq!(
        evicted_txids(&pool).await,
        vec!["y"],
        "the first flush must evict only y, or this is not the evicted-then-mined split"
    );

    let fees = HashMap::from([("x".to_string(), TX_FEE)]);
    let sizes = HashMap::from([("x".to_string(), TX_VSIZE)]);
    deps.cluster_service()
        .confirm_mined(&["x".into()], &fees, &sizes, fixed_time())
        .await;

    deps.mempool_ledger.submit_authoritative(HashSet::new());
    reconciler.tick().await;

    let confirmed = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        confirmed.status,
        ClusterStatus::Confirmed,
        "a cluster with a mined member must end confirmed, not evicted, even though \
         its other member was evicted first"
    );
    assert!(
        confirmed.confirmed_at.is_some(),
        "a confirmed cluster must carry confirmed_at, or the UI keeps showing it as pending"
    );
    assert_eq!(
        confirmed.txids,
        vec!["x".to_string()],
        "a confirmed cluster must hold only the members that made it into the block"
    );
    assert!(
        deps.repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "no member is left in the mempool, so nothing may remain in the active set"
    );
    assert!(
        deps.repos
            .cluster
            .find_by_txid("y")
            .await
            .expect("query")
            .is_none(),
        "an evicted member of a partially mined cluster ends in no cluster at all; its \
         membership survives only in cluster_deltas"
    );
}

#[tokio::test]
async fn a_survivor_the_node_still_groups_keeps_the_cluster_and_confirms_it_alone() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["x", "y"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["x".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("x")
        .await
        .expect("query")
        .expect("cluster exists");

    retriever.set_clusters(vec![ClusterFixture::new(&["x"]).build()]);
    deps.mempool_ledger
        .submit_authoritative(HashSet::from(["x".to_string()]));
    reconciler.tick().await;
    assert_eq!(
        evicted_txids(&pool).await,
        vec!["y"],
        "the first flush must evict only y"
    );

    let shrunk = deps.repos.cluster.find_active().await.expect("active");
    assert_eq!(
        shrunk.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![stored.id],
        "the survivor stays in the cluster it already had, not in a new one"
    );
    assert_eq!(
        shrunk[0].txids,
        vec!["x".to_string()],
        "the cluster must shrink to the group the node still reports"
    );
    assert!(
        deps.repos
            .cluster
            .find_by_txid("y")
            .await
            .expect("query")
            .is_none(),
        "the evicted member leaves with no cluster of its own"
    );

    let fees = HashMap::from([("x".to_string(), TX_FEE)]);
    let sizes = HashMap::from([("x".to_string(), TX_VSIZE)]);
    deps.cluster_service()
        .confirm_mined(&["x".into()], &fees, &sizes, fixed_time())
        .await;

    let confirmed = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        confirmed.status,
        ClusterStatus::Confirmed,
        "a block mining the whole shrunk cluster must confirm it"
    );
    assert_eq!(
        confirmed.txids,
        vec!["x".to_string()],
        "confirming must keep the shrunk membership"
    );
    assert!(
        deps.repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "no member is left in the mempool, so nothing may remain in the active set"
    );
}

#[tokio::test]
async fn a_mined_member_flushed_before_its_block_is_applied_closes_the_cluster_as_evicted() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["x", "y"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["x".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("x")
        .await
        .expect("query")
        .expect("cluster exists");

    retriever.set_clusters(vec![]);
    deps.mempool_ledger
        .submit_authoritative(HashSet::from(["x".to_string()]));
    reconciler.tick().await;

    // The flush reaches x before its block is applied, so nothing yet tells
    // it x was mined. Accepted: the cluster's status then agrees with x's own
    // remove_evicted delta, which is also what the retroactive cleanup reads.
    deps.mempool_ledger.submit_authoritative(HashSet::new());
    reconciler.tick().await;
    assert_eq!(
        evicted_txids(&pool).await,
        vec!["x", "y"],
        "x must reach a flush as evicted before its block, or this is not the race"
    );

    let fees = HashMap::from([("x".to_string(), TX_FEE)]);
    let sizes = HashMap::from([("x".to_string(), TX_VSIZE)]);
    deps.cluster_service()
        .confirm_mined(&["x".into()], &fees, &sizes, fixed_time())
        .await;

    let closed = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        closed.status,
        ClusterStatus::Evicted,
        "once every member has left the ledger the cluster closes as evicted, and a \
         block applied afterwards does not reopen it"
    );
    assert_eq!(
        closed.txids,
        vec!["x".to_string(), "y".to_string()],
        "an evicted cluster keeps its full membership"
    );
    assert!(
        deps.repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "no member is left in the mempool, so nothing may remain in the active set"
    );
}

#[tokio::test]
async fn the_flush_confirms_a_cluster_its_block_never_got_to_confirm() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");

    let mined_at = fixed_time() + Duration::minutes(10);
    commit_block_without_confirming_clusters(&deps.repos, "block-1", 1, &["a", "b"], mined_at)
        .await;

    retriever.set_clusters(vec![]);
    deps.mempool_ledger.submit_authoritative(HashSet::new());
    reconciler.tick().await;

    for txid in ["a", "b"] {
        assert_eq!(
            delta_reasons(&pool, txid).await,
            vec![DeltaReason::AddMempool, DeltaReason::RemoveConfirmed],
            "{txid} is mined, so the flush must record its removal as confirmed"
        );
    }

    let confirmed = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        confirmed.status,
        ClusterStatus::Confirmed,
        "the block's cluster confirmation never ran, so the flush recording the mined \
         members' removal must close the cluster itself"
    );
    assert_eq!(
        confirmed.confirmed_at,
        Some(mined_at),
        "the cluster must be confirmed at its members' block time, not at flush time"
    );
    assert_eq!(
        confirmed.txids,
        vec!["a".to_string(), "b".to_string()],
        "every member made it into the block, so the membership stays whole"
    );
    assert!(
        deps.repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "no member is left in the mempool, so nothing may remain in the active set"
    );
}

#[tokio::test]
async fn a_cluster_mined_in_one_block_stays_whole_when_its_members_leave_in_separate_flushes() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");

    let mined_at = fixed_time() + Duration::minutes(10);
    commit_block_without_confirming_clusters(&deps.repos, "block-1", 1, &["a", "b"], mined_at)
        .await;

    retriever.set_clusters(vec![]);
    deps.mempool_ledger
        .submit_authoritative(HashSet::from(["b".to_string()]));
    reconciler.tick().await;
    assert_eq!(
        delta_reasons(&pool, "b").await,
        vec![DeltaReason::AddMempool],
        "the flush recording a's removal must leave b in the mempool, or this is not the split"
    );

    let confirmed = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        confirmed.status,
        ClusterStatus::Confirmed,
        "the flush recording a's removal must close the cluster"
    );
    assert_eq!(
        confirmed.txids,
        vec!["a".to_string(), "b".to_string()],
        "b was mined in the same block, so it must not be trimmed out as if still pending"
    );
    let deltas_after_close = cluster_delta_count(&pool, stored.id).await;

    deps.mempool_ledger.submit_authoritative(HashSet::new());
    reconciler.tick().await;
    assert_eq!(
        delta_reasons(&pool, "b").await,
        vec![DeltaReason::AddMempool, DeltaReason::RemoveConfirmed],
        "b's own removal must reach a later flush, labelled confirmed"
    );

    let after = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        (after.status, after.txids, after.confirmed_at),
        (confirmed.status, confirmed.txids, confirmed.confirmed_at),
        "b's later removal must leave the already confirmed cluster as it was"
    );
    assert_eq!(
        cluster_delta_count(&pool, stored.id).await,
        deltas_after_close,
        "a cluster already closed must not be logged again"
    );
}

#[tokio::test]
async fn a_partly_mined_cluster_whose_confirmation_never_ran_regroups_its_pending_member() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["p", "c"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["p".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("p")
        .await
        .expect("query")
        .expect("cluster exists");

    let mined_at = fixed_time() + Duration::minutes(10);
    commit_block_without_confirming_clusters(&deps.repos, "block-1", 1, &["p"], mined_at).await;

    retriever.set_clusters(vec![ClusterFixture::new(&["c"]).build()]);
    deps.mempool_ledger
        .submit_authoritative(HashSet::from(["c".to_string()]));
    reconciler.tick().await;

    let confirmed = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        confirmed.status,
        ClusterStatus::Confirmed,
        "the mined part of the cluster must be confirmed by the flush"
    );
    assert_eq!(
        confirmed.txids,
        vec!["p".to_string()],
        "only the mined member stays in the confirmed cluster"
    );
    assert_eq!(confirmed.confirmed_at, Some(mined_at));

    let active = deps.repos.cluster.find_active().await.expect("active");
    assert_eq!(
        active.iter().map(|c| c.txids.clone()).collect::<Vec<_>>(),
        vec![vec!["c".to_string()]],
        "the still-pending member must land in a cluster of its own, as the node now groups it"
    );
    assert_ne!(
        active[0].id, stored.id,
        "the pending member gets a new cluster, not the confirmed one"
    );
}

#[tokio::test]
async fn unconfirmed_blocks_are_confirmed_in_height_order_not_by_their_stamped_time() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["p", "c"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["p".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("p")
        .await
        .expect("query")
        .expect("cluster exists");

    // the later block carries the earlier timestamp, which consensus allows
    let first_block_at = fixed_time() + Duration::minutes(10);
    let second_block_at = fixed_time() + Duration::minutes(5);
    commit_block_without_confirming_clusters(&deps.repos, "block-1", 1, &["p"], first_block_at)
        .await;
    commit_block_without_confirming_clusters(&deps.repos, "block-2", 2, &["c"], second_block_at)
        .await;

    retriever.set_clusters(vec![]);
    deps.mempool_ledger.submit_authoritative(HashSet::new());
    reconciler.tick().await;

    let confirmed = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        (confirmed.txids, confirmed.confirmed_at),
        (vec!["p".to_string()], Some(first_block_at)),
        "the lower block must be confirmed first, as apply_block would have, so the \
         cluster keeps the member of the block that mined first"
    );
    assert!(
        deps.repos
            .cluster
            .find_by_txid("c")
            .await
            .expect("query")
            .is_none(),
        "the node no longer knows c, so nothing can regroup it; its membership survives \
         only in cluster_deltas"
    );
}

#[tokio::test]
async fn a_mined_and_an_evicted_member_leaving_in_one_flush_confirm_the_cluster_as_the_mined_one() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever.clone());
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");

    let mined_at = fixed_time() + Duration::minutes(10);
    commit_block_without_confirming_clusters(&deps.repos, "block-1", 1, &["a"], mined_at).await;

    retriever.set_clusters(vec![]);
    deps.mempool_ledger.submit_authoritative(HashSet::new());
    reconciler.tick().await;
    assert_eq!(
        evicted_txids(&pool).await,
        vec!["b"],
        "only b must be written as evicted, or this is not the mixed flush"
    );

    let confirmed = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        confirmed.status,
        ClusterStatus::Confirmed,
        "a cluster with a mined member must end confirmed, even when its other member \
         is evicted in the same flush"
    );
    assert_eq!(confirmed.confirmed_at, Some(mined_at));
    assert_eq!(
        confirmed.txids,
        vec!["a".to_string()],
        "a confirmed cluster must hold only the members that made it into the block"
    );
    assert_eq!(
        (confirmed.total_fee, confirmed.total_vsize),
        (TX_FEE, TX_VSIZE),
        "the totals must shrink with the membership"
    );
    assert!(
        deps.repos
            .cluster
            .find_by_txid("b")
            .await
            .expect("query")
            .is_none(),
        "an evicted member of a partially mined cluster ends in no cluster at all"
    );
    assert!(
        deps.repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty(),
        "no member is left in the mempool, so nothing may remain in the active set"
    );
}

#[tokio::test]
async fn a_cluster_its_block_already_confirmed_is_untouched_by_the_flush_of_its_removals() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::strict(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let block = BlockFixture::new("block-1", 1)
        .with_mined_at(fixed_time() + Duration::minutes(10))
        .with_txs(&[("a", TX_FEE), ("b", TX_FEE)])
        .build();
    let deps = deps(pool.clone())
        .with_cluster_retriever(retriever.clone())
        .with_block_retriever(MockBlockRetriever::with_blocks(vec![block]));
    let reconciler = deps.mempool_reconciler();

    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;
    reconciler.tick().await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");

    retriever.set_clusters(vec![]);
    deps.block_service()
        .apply_block(BlockConnectedEvent {
            hash: "block-1".into(),
        })
        .await
        .expect("apply block");
    let before = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(before.status, ClusterStatus::Confirmed);
    let deltas_before = cluster_delta_count(&pool, stored.id).await;

    // apply_block already queued the removals; this flush writes them
    reconciler.tick().await;
    for txid in ["a", "b"] {
        assert_eq!(
            delta_reasons(&pool, txid).await,
            vec![DeltaReason::AddMempool, DeltaReason::RemoveConfirmed],
            "{txid} must reach the flush as confirmed, or this is not the normal path"
        );
    }

    let after = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(
        (after.status, after.confirmed_at, after.txids),
        (before.status, before.confirmed_at, before.txids),
        "a cluster its block already confirmed must be left exactly as it was"
    );
    assert_eq!(
        cluster_delta_count(&pool, stored.id).await,
        deltas_before,
        "a cluster its block already confirmed must gain no cluster_deltas row"
    );
}

/// Commits a block's rows the way `apply_block` does, then stops short of
/// confirming the clusters its txs belonged to.
async fn commit_block_without_confirming_clusters(
    repos: &Repos,
    hash: &str,
    height: i64,
    txids: &[&str],
    mined_at: OffsetDateTime,
) {
    let txs: Vec<NewTransaction> = txids
        .iter()
        .map(|txid| {
            TxFixture::new(txid)
                .sized()
                .with_confirmed_at(Some(mined_at))
                .with_confirmed_at_block(hash)
                .build()
        })
        .collect();
    repos
        .block
        .insert_with_transactions(
            &NewBlockFixture::new(hash, height)
                .with_mined_at(mined_at)
                .build(),
            &txs,
        )
        .await
        .expect("commit block");
}

async fn delta_reasons(pool: &DbPool, txid: &str) -> Vec<DeltaReason> {
    let mut conn = pool.get().await.expect("checkout connection");
    mempool_deltas::table
        .filter(mempool_deltas::txid.eq(txid))
        .order(mempool_deltas::id.asc())
        .select(mempool_deltas::reason)
        .load(&mut conn)
        .await
        .expect("load delta reasons")
}

async fn cluster_delta_count(pool: &DbPool, cluster_id: i64) -> i64 {
    let mut conn = pool.get().await.expect("checkout connection");
    cluster_deltas::table
        .filter(cluster_deltas::cluster_id.eq(cluster_id))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count cluster deltas")
}

async fn evicted_txids(pool: &DbPool) -> Vec<String> {
    let mut conn = pool.get().await.expect("checkout connection");
    mempool_deltas::table
        .filter(mempool_deltas::reason.eq(DeltaReason::RemoveEvicted))
        .order(mempool_deltas::txid.asc())
        .select(mempool_deltas::txid)
        .load(&mut conn)
        .await
        .expect("load evicted txids")
}

#[tokio::test]
async fn a_hollow_insert_never_downgrades_a_row_that_already_carries_fee_and_vsize() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());

    tx_repo
        .insert(&TxFixture::new("a").sized().build())
        .await
        .expect("seed sized row");

    // the vehicle here is `insert_many`, the same upsert bootstrap and the
    // reconciler's `insert_hollow_transactions` use: on_conflict(txid).do_nothing()
    // leaves an already-existing row alone, hollow or not.
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

#[tokio::test]
async fn a_member_the_delta_path_already_admitted_gets_no_second_add_event() {
    let pool = isolated_pool().await;
    let retriever =
        MockClusterRetriever::with_clusters(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever);
    let reconciler = deps.mempool_reconciler();

    // "a" arrives the normal way: getrawmempool reports it, the ledger queues
    // the Add, and the reconciler writes the row and the event together
    deps.mempool_ledger
        .submit_authoritative(HashSet::from(["a".to_string()]));
    reconciler.tick().await;

    // the node then groups it with "b", which we have never seen
    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;
    reconciler.tick().await;

    let mut conn = pool.get().await.expect("checkout connection");
    for (txid, admitted_by) in [("a", "the delta path"), ("b", "the cluster answer")] {
        let adds: i64 = mempool_deltas::table
            .filter(mempool_deltas::txid.eq(txid))
            .filter(mempool_deltas::reason.eq(DeltaReason::AddMempool))
            .count()
            .get_result(&mut conn)
            .await
            .expect("count add events");
        assert_eq!(
            adds, 1,
            "{txid}, admitted by {admitted_by}, must carry exactly one add event"
        );
    }
}

#[tokio::test]
async fn a_member_the_cluster_path_already_admitted_gets_no_second_add_event() {
    let pool = isolated_pool().await;
    let retriever =
        MockClusterRetriever::with_clusters(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool.clone()).with_cluster_retriever(retriever);
    let reconciler = deps.mempool_reconciler();

    // the node's getmempoolcluster answer arrives first: handle_candidates_covering
    // asserts "a" and "b" present, and the reconciler's flush writes one
    // add_mempool event for each
    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;
    reconciler.tick().await;

    // ~10s later getrawmempool diffs and still reports both as resident: the
    // ledger already holds them in `live`, so submit_authoritative queues no
    // new Add for either
    deps.mempool_ledger
        .submit_authoritative(HashSet::from(["a".to_string(), "b".to_string()]));
    reconciler.tick().await;

    let mut conn = pool.get().await.expect("checkout connection");
    for txid in ["a", "b"] {
        let adds: i64 = mempool_deltas::table
            .filter(mempool_deltas::txid.eq(txid))
            .filter(mempool_deltas::reason.eq(DeltaReason::AddMempool))
            .count()
            .get_result(&mut conn)
            .await
            .expect("count add events");
        assert_eq!(
            adds, 1,
            "{txid}, admitted by the cluster answer, must carry exactly one add event: the \
             ledger must not queue a second Add when the delta path's getrawmempool poll \
             re-reports a txid the cluster path already asserted present"
        );
    }
}
