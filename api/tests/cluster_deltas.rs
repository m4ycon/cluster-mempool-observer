#![cfg(feature = "db_integration_tests")]

use api::db::models::ClusterDelta;
use api::db::schema::cluster_deltas;
use api::db::{DbPool, Repos, TransactionRepository};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use futures::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use testkit::deps::{cluster_service, deps, strict_cluster_service};
use testkit::fixtures::{
    ClusterFixture, MempoolDeltaEventFixture, MempoolDeltaFixture, TX_FEE, TX_VSIZE, fixed_time,
    seed_sized_txs,
};
use testkit::mocks::{MockClusterRetriever, MockTransactionRetriever};
use testkit::postgres::isolated_pool;

async fn delta_rows(pool: &DbPool) -> Vec<ClusterDelta> {
    let mut conn = pool.get().await.expect("conn");
    cluster_deltas::table
        .order(cluster_deltas::id.asc())
        .select(ClusterDelta::as_select())
        .load(&mut conn)
        .await
        .expect("load cluster deltas")
}

fn sorted(txids: &[String]) -> Vec<String> {
    let mut v = txids.to_vec();
    v.sort();
    v
}

/// Every cluster's life in log-space must end balanced: deltas sum to zero
/// and the membership fold (adds minus removes) ends empty.
fn assert_delta_zero(rows: &[ClusterDelta], cluster_id: i64) {
    let rows: Vec<&ClusterDelta> = rows.iter().filter(|r| r.cluster_id == cluster_id).collect();
    assert!(!rows.is_empty(), "no log rows for cluster {cluster_id}");

    let fee_sum: i64 = rows.iter().map(|r| r.fee_delta).sum();
    let vsize_sum: i64 = rows.iter().map(|r| r.vsize_delta).sum();
    assert_eq!(
        fee_sum, 0,
        "fee deltas of cluster {cluster_id} must sum to 0"
    );
    assert_eq!(
        vsize_sum, 0,
        "vsize deltas of cluster {cluster_id} must sum to 0"
    );

    let mut members: Vec<String> = Vec::new();
    for row in rows {
        members.extend(row.added_txids.iter().cloned());
        members.retain(|txid| !row.removed_txids.contains(txid));
    }
    assert!(
        members.is_empty(),
        "membership fold of cluster {cluster_id} left {members:?}"
    );
}

#[tokio::test]
async fn new_cluster_logs_added_members_and_positive_deltas() {
    let pool = isolated_pool().await;
    let deps =
        deps(pool.clone()).with_cluster_retriever(MockClusterRetriever::with_clusters(vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1500)
                .build(),
        ]));
    seed_sized_txs(&deps.repos.transaction, &["a", "b"]).await;

    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;

    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cluster_id, stored.id);
    assert_eq!(sorted(&rows[0].added_txids), vec!["a", "b"]);
    assert!(rows[0].removed_txids.is_empty());
    assert_eq!(rows[0].fee_delta, 1500);
    assert_eq!(rows[0].vsize_delta, 2 * TX_VSIZE);
}

#[tokio::test]
async fn growth_logs_only_the_joined_member() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    seed_sized_txs(&tx_repo, &["a", "b", "c"]).await;

    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1000)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;
    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b", "c"])
                .with_total_fee_sats(1500)
                .build(),
        ],
    )
    .sync_clusters_for(&["c".into()], &[])
    .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].added_txids, vec!["c"]);
    assert!(rows[1].removed_txids.is_empty());
    assert_eq!(rows[1].fee_delta, 500);
    assert_eq!(rows[1].vsize_delta, TX_VSIZE);
}

#[tokio::test]
async fn departure_logs_only_the_removed_member() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    seed_sized_txs(&tx_repo, &["a", "b", "c"]).await;

    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b", "c"])
                .with_total_fee_sats(1500)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;
    // strict: c left the mempool altogether, so nothing re-clusters it here
    strict_cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1000)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert_eq!(rows[1].removed_txids, vec!["c"]);
    assert_eq!(rows[1].fee_delta, -500);
    assert_eq!(rows[1].vsize_delta, -TX_VSIZE);
}

#[tokio::test]
async fn unchanged_resync_logs_nothing() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    seed_sized_txs(&tx_repo, &["a", "b"]).await;

    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1500)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;
    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1500)
                .build(),
        ],
    )
    .sync_clusters_for(&["b".into()], &[])
    .await;

    assert_eq!(delta_rows(&pool).await.len(), 1);
}

#[tokio::test]
async fn merge_closes_loser_before_keeper_absorbs_members() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b", "c", "d"]).await;

    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1000)
                .build(),
            ClusterFixture::new(&["c", "d"])
                .with_total_fee_sats(800)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into(), "c".into()], &[])
    .await;
    let keeper = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    let loser = repos
        .cluster
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");

    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b", "c", "d"])
                .with_total_fee_sats(1800)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 4);

    // loser's closing row comes before the keeper's absorption row
    assert_eq!(rows[2].cluster_id, loser.id);
    assert!(rows[2].added_txids.is_empty());
    assert_eq!(sorted(&rows[2].removed_txids), vec!["c", "d"]);
    assert_eq!(rows[2].fee_delta, -800);
    assert_eq!(rows[2].vsize_delta, -2 * TX_VSIZE);

    assert_eq!(rows[3].cluster_id, keeper.id);
    assert_eq!(sorted(&rows[3].added_txids), vec!["c", "d"]);
    assert!(rows[3].removed_txids.is_empty());
    assert_eq!(rows[3].fee_delta, 800);
    assert_eq!(rows[3].vsize_delta, 2 * TX_VSIZE);

    assert_delta_zero(&rows, loser.id);
    let active = repos.cluster.find_active().await.expect("active");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, keeper.id);
}

#[tokio::test]
async fn full_confirm_logs_closing_row_once() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b"]).await;

    let svc = cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1500)
                .build(),
        ],
    );
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let stored = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    let fees = HashMap::from([("a".to_string(), 800i64), ("b".to_string(), 700i64)]);
    let sizes = HashMap::from([("a".to_string(), 100i64), ("b".to_string(), 100i64)]);
    let mined: Vec<String> = vec!["a".into(), "b".into()];
    svc.confirm_mined(&mined, &fees, &sizes, fixed_time()).await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert_eq!(sorted(&rows[1].removed_txids), vec!["a", "b"]);
    assert_eq!(rows[1].fee_delta, -1500);
    assert_eq!(rows[1].vsize_delta, -2 * TX_VSIZE);
    assert_delta_zero(&rows, stored.id);

    // re-confirming must not log a second closing row
    svc.confirm_mined(&mined, &fees, &sizes, fixed_time()).await;
    assert_eq!(delta_rows(&pool).await.len(), 2);

    // confirmed member txs keep their cluster back-link
    assert_eq!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&mined)
            .await
            .expect("ids"),
        vec![stored.id]
    );
}

#[tokio::test]
async fn partial_confirm_logs_departures_then_closing_row() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b", "c", "d"]).await;

    // initial cluster {a,b,c,d}; after {a,b} confirm, the retriever reports
    // the still-pending {c,d} as their own cluster
    let svc = cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b", "c", "d"])
                .with_total_fee_sats(1800)
                .build(),
            ClusterFixture::new(&["c", "d"])
                .with_total_fee_sats(800)
                .build(),
        ],
    );
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let original = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    let fees = HashMap::from([("a".to_string(), 600i64), ("b".to_string(), 400i64)]);
    let sizes = HashMap::from([("a".to_string(), TX_VSIZE), ("b".to_string(), TX_VSIZE)]);
    svc.confirm_mined(&["a".into(), "b".into()], &fees, &sizes, fixed_time())
        .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 4);

    // still-pending members leave first, with the confirmed-subset totals
    assert_eq!(rows[1].cluster_id, original.id);
    assert_eq!(sorted(&rows[1].removed_txids), vec!["c", "d"]);
    assert_eq!(rows[1].fee_delta, -800);
    assert_eq!(rows[1].vsize_delta, -2 * TX_VSIZE);

    // then the confirm closes the original cluster
    assert_eq!(rows[2].cluster_id, original.id);
    assert_eq!(sorted(&rows[2].removed_txids), vec!["a", "b"]);
    assert_eq!(rows[2].fee_delta, -1000);
    assert_eq!(rows[2].vsize_delta, -2 * TX_VSIZE);
    assert_delta_zero(&rows, original.id);

    // the pending pair gets its own cluster with its own opening row
    let pending = repos
        .cluster
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    assert_ne!(pending.id, original.id);
    assert_eq!(rows[3].cluster_id, pending.id);
    assert_eq!(sorted(&rows[3].added_txids), vec!["c", "d"]);
    assert_eq!(rows[3].fee_delta, 800);
    assert_eq!(rows[3].vsize_delta, 2 * TX_VSIZE);
}

#[tokio::test]
async fn fee_only_change_logs_empty_arrays_with_fee_delta() {
    // impossible case in practice, but the cluster service
    // should still log a delta row if the total fee changes
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    seed_sized_txs(&tx_repo, &["a", "b"]).await;

    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1000)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;
    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1500)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert!(rows[1].removed_txids.is_empty());
    assert_eq!(rows[1].fee_delta, 500);
    assert_eq!(rows[1].vsize_delta, 0);
}

#[tokio::test]
async fn eviction_reshapes_the_cluster_from_the_node() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b", "c"]).await;

    let retriever = MockClusterRetriever::with_clusters(vec![
        ClusterFixture::new(&["a", "b", "c"])
            .with_total_fee_sats((3 * TX_FEE) as u64)
            .build(),
    ]);
    let svc = deps(pool.clone())
        .with_cluster_retriever(retriever.clone())
        .cluster_service();
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let stored = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    // the node stops reporting the evicted tx as part of the group
    retriever.set_clusters(vec![
        ClusterFixture::new(&["a", "b"])
            .with_total_fee_sats((2 * TX_FEE) as u64)
            .build(),
    ]);
    svc.sync_clusters_for(&[], &["c".into()]).await;

    // membership and totals come back from the node, not from a local recompute
    let shrunk = repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(sorted(&shrunk.txids), vec!["a", "b"]);
    assert_eq!(shrunk.total_fee, 2 * TX_FEE);
    assert_eq!(shrunk.total_vsize, 2 * TX_VSIZE);

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert_eq!(rows[1].removed_txids, vec!["c"]);
    assert_eq!(rows[1].fee_delta, -TX_FEE);
    assert_eq!(rows[1].vsize_delta, -TX_VSIZE);

    // evicted tx detached, cluster still active
    assert!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&["c".into()])
            .await
            .expect("ids")
            .is_empty()
    );
    assert_eq!(repos.cluster.find_active().await.expect("active").len(), 1);
}

#[tokio::test]
async fn eviction_leaves_the_survivor_in_a_cluster_of_its_own() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b"]).await;

    let retriever = MockClusterRetriever::with_clusters(vec![
        ClusterFixture::new(&["a", "b"])
            .with_total_fee_sats(1000)
            .build(),
    ]);
    let svc = deps(pool.clone())
        .with_cluster_retriever(retriever.clone())
        .cluster_service();
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let stored = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    retriever.set_clusters(vec![
        ClusterFixture::new(&["a"]).with_total_fee_sats(500).build(),
    ]);
    svc.sync_clusters_for(&[], &["b".into()]).await;

    // a is alone now, but alone is a cluster: the row shrinks instead of closing
    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert_eq!(rows[1].removed_txids, vec!["b"]);
    assert_eq!(rows[1].fee_delta, -500);
    assert_eq!(rows[1].vsize_delta, -TX_VSIZE);

    let shrunk = repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(shrunk.txids, vec!["a".to_string()]);
    assert_eq!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&["a".into()])
            .await
            .expect("ids"),
        vec![stored.id]
    );
    assert!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&["b".into()])
            .await
            .expect("ids")
            .is_empty()
    );
    let active = repos.cluster.find_active().await.expect("active");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, stored.id);
}

#[tokio::test]
async fn eviction_of_every_member_closes_the_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b"]).await;

    let svc = cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1000)
                .build(),
        ],
    );
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let stored = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    svc.sync_clusters_for(&[], &["a".into(), "b".into()]).await;

    // nothing left to belong to the cluster, so it closes
    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert_eq!(sorted(&rows[1].removed_txids), vec!["a", "b"]);
    assert_eq!(rows[1].fee_delta, -1000);
    assert_eq!(rows[1].vsize_delta, -2 * TX_VSIZE);
    assert_delta_zero(&rows, stored.id);

    let closed = repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert!(closed.txids.is_empty());
    assert!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&["a".into(), "b".into()])
            .await
            .expect("ids")
            .is_empty()
    );
    assert!(
        repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty()
    );
}

#[tokio::test]
async fn evicting_the_middle_member_splits_the_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b", "c"]).await;

    let retriever = MockClusterRetriever::with_clusters(vec![
        ClusterFixture::new(&["a", "b", "c"])
            .with_total_fee_sats((3 * TX_FEE) as u64)
            .build(),
    ]);
    let svc = deps(pool.clone())
        .with_cluster_retriever(retriever.clone())
        .cluster_service();
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let stored = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    // b was the only link between a and c
    retriever.set_clusters(vec![
        ClusterFixture::new(&["a"]).build(),
        ClusterFixture::new(&["c"]).build(),
    ]);
    svc.sync_clusters_for(&[], &["b".into()]).await;

    let active = repos.cluster.find_active().await.expect("active");
    let mut members: Vec<Vec<String>> = active.iter().map(|c| sorted(&c.txids)).collect();
    members.sort();
    assert_eq!(members, vec![vec!["a".to_string()], vec!["c".to_string()]]);

    // the original row keeps one side, the other side gets a row of its own
    assert!(active.iter().any(|c| c.id == stored.id));
    assert!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&["b".into()])
            .await
            .expect("ids")
            .is_empty(),
        "the evicted tx must not stay linked"
    );
    for txid in ["a", "c"] {
        assert_eq!(
            repos
                .transaction
                .get_cluster_ids_by_txids(&[txid.into()])
                .await
                .expect("ids")
                .len(),
            1,
            "{txid} must belong to exactly one cluster"
        );
    }
}

#[tokio::test]
async fn eviction_skips_confirmed_clusters() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b"]).await;

    let svc = cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1500)
                .build(),
        ],
    );
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let fees = HashMap::from([("a".to_string(), 800i64), ("b".to_string(), 700i64)]);
    let sizes = HashMap::from([("a".to_string(), 100i64), ("b".to_string(), 100i64)]);
    svc.confirm_mined(&["a".into(), "b".into()], &fees, &sizes, fixed_time())
        .await;
    assert_eq!(delta_rows(&pool).await.len(), 2);

    // confirmed members keep their back-link; eviction must not touch the cluster
    svc.sync_clusters_for(&[], &["a".into()]).await;

    assert_eq!(delta_rows(&pool).await.len(), 2);
    let confirmed = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("row kept with members");
    assert_eq!(confirmed.txids.len(), 2);
    assert!(confirmed.confirmed_at.is_some());
}

#[tokio::test]
async fn mempool_eviction_flows_into_cluster_shrink_and_ws_frame() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::with_clusters(vec![
        ClusterFixture::new(&["a", "b", "c"])
            .with_total_fee_sats(1500)
            .build(),
    ]);
    let deps = deps(pool.clone())
        .with_cluster_retriever(retriever.clone())
        .with_transaction_retriever(MockTransactionRetriever::default());
    seed_sized_txs(&deps.repos.transaction, &["a", "b", "c"]).await;

    // pre-build the cluster, then drive an eviction through the mempool service
    let svc = deps.cluster_service();
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    // the eviction only pairs against a recorded mempool entry
    deps.repos
        .mempool_delta
        .insert_many(&[MempoolDeltaFixture::added("c").build()])
        .await
        .expect("seed add delta");

    retriever.set_clusters(vec![
        ClusterFixture::new(&["a", "b"])
            .with_total_fee_sats(1000)
            .build(),
    ]);

    let mut frames = Box::pin(svc.get_delta_stream().await);
    let mempool_service = deps.mempool_service();

    let source = futures::stream::iter(vec![
        MempoolDeltaEventFixture::new().with_removed(&["c"]).build(),
    ]);
    mempool_service.persist_deltas_and_new_txs(source).await;

    let shrunk = deps
        .repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(sorted(&shrunk.txids), vec!["a", "b"]);

    // websocket subscribers see the shrink as an upserted frame
    let frame = tokio::time::timeout(Duration::from_secs(1), frames.next())
        .await
        .expect("frame within timeout")
        .expect("stream open");
    assert_eq!(frame.upserted.len(), 1);
    assert_eq!(frame.upserted[0].id, stored.id);
    assert_eq!(sorted(&frame.upserted[0].txids), vec!["a", "b"]);
}
