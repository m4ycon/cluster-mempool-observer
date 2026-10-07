#![cfg(feature = "db_integration_tests")]

use api::db::models::{ClusterDelta, ClusterStatus, NewCluster};
use api::db::schema::{cluster_deltas, transactions};
use api::db::{ClusterMembershipUpdate, DbPool, Repos, TransactionRepository};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use futures::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use testkit::deps::{cluster_service, deps, strict_cluster_service};
use testkit::fixtures::{ClusterFixture, TX_FEE, TX_WEIGHT, fixed_time, seed_sized_txs};
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
    let weight_sum: i64 = rows.iter().map(|r| r.weight_delta).sum();
    assert_eq!(
        fee_sum, 0,
        "fee deltas of cluster {cluster_id} must sum to 0"
    );
    assert_eq!(
        weight_sum, 0,
        "weight deltas of cluster {cluster_id} must sum to 0"
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
    assert_eq!(rows[0].weight_delta, 2 * TX_WEIGHT);
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
    assert_eq!(rows[1].weight_delta, TX_WEIGHT);
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
    assert_eq!(rows[1].weight_delta, -TX_WEIGHT);
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
    assert_eq!(rows[2].weight_delta, -2 * TX_WEIGHT);

    assert_eq!(rows[3].cluster_id, keeper.id);
    assert_eq!(sorted(&rows[3].added_txids), vec!["c", "d"]);
    assert!(rows[3].removed_txids.is_empty());
    assert_eq!(rows[3].fee_delta, 800);
    assert_eq!(rows[3].weight_delta, 2 * TX_WEIGHT);

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
    let weights = HashMap::from([("a".to_string(), TX_WEIGHT), ("b".to_string(), TX_WEIGHT)]);
    let mined: Vec<String> = vec!["a".into(), "b".into()];
    svc.confirm_mined(&mined, &fees, &weights, fixed_time())
        .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert_eq!(sorted(&rows[1].removed_txids), vec!["a", "b"]);
    assert_eq!(rows[1].fee_delta, -1500);
    assert_eq!(rows[1].weight_delta, -2 * TX_WEIGHT);
    assert_delta_zero(&rows, stored.id);

    // re-confirming must not log a second closing row
    svc.confirm_mined(&mined, &fees, &weights, fixed_time())
        .await;
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

async fn insert_bare_cluster(repos: &Repos, txids: &[&str]) -> i64 {
    repos
        .cluster
        .insert(&NewCluster {
            txids: txids.iter().map(|t| t.to_string()).collect(),
            total_weight: txids.len() as i64 * TX_WEIGHT,
            total_fee: txids.len() as i64 * TX_FEE,
            first_seen_at: fixed_time(),
        })
        .await
        .expect("insert")
        .id
}

#[tokio::test]
async fn confirm_many_closes_each_cluster_with_its_own_row() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let ab = insert_bare_cluster(&repos, &["a", "b"]).await;
    let c = insert_bare_cluster(&repos, &["c"]).await;
    let empty = insert_bare_cluster(&repos, &[]).await;

    let confirmed = repos
        .cluster_membership
        .confirm_many(&[c, empty, ab], fixed_time())
        .await
        .expect("confirm_many");
    assert_eq!(confirmed, 3);

    for row in repos
        .cluster
        .find_by_ids(&[ab, c, empty])
        .await
        .expect("load")
    {
        assert_eq!(row.status, ClusterStatus::Confirmed, "cluster {}", row.id);
        assert_eq!(row.confirmed_at, Some(fixed_time()), "cluster {}", row.id);
    }

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2, "an empty cluster has no members to close");
    for (id, members) in [(ab, vec!["a", "b"]), (c, vec!["c"])] {
        let row = rows
            .iter()
            .find(|r| r.cluster_id == id)
            .expect("closing row");
        assert_eq!(sorted(&row.removed_txids), members);
        assert!(row.added_txids.is_empty());
        assert_eq!(row.fee_delta, -(members.len() as i64) * TX_FEE);
        assert_eq!(row.weight_delta, -(members.len() as i64) * TX_WEIGHT);
    }
}

#[tokio::test]
async fn confirm_many_leaves_an_already_confirmed_cluster_alone() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let ab = insert_bare_cluster(&repos, &["a", "b"]).await;
    let c = insert_bare_cluster(&repos, &["c"]).await;
    let earlier = fixed_time() - time::Duration::minutes(10);
    repos
        .cluster_membership
        .confirm_many(&[ab], earlier)
        .await
        .expect("confirm");

    let confirmed = repos
        .cluster_membership
        .confirm_many(&[ab, c], fixed_time())
        .await
        .expect("confirm_many");
    assert_eq!(confirmed, 1);

    let rows = delta_rows(&pool).await;
    assert_eq!(
        rows.iter().filter(|r| r.cluster_id == ab).count(),
        1,
        "re-confirming logged a second closing row"
    );
    let ab_row = repos
        .cluster
        .find_by_ids(&[ab])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(
        ab_row.confirmed_at,
        Some(earlier),
        "first confirmation overwritten"
    );
}

async fn cluster_link(pool: &DbPool, txid: &str) -> Option<i64> {
    let mut conn = pool.get().await.expect("conn");
    transactions::table
        .filter(transactions::txid.eq(txid))
        .select(transactions::cluster_id)
        .first(&mut conn)
        .await
        .expect("tx row")
}

#[tokio::test]
async fn trim_and_confirm_many_keeps_only_the_given_members_of_each_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let mut ids = Vec::new();
    for txids in [vec!["a", "b", "c"], vec!["d", "e"]] {
        let cluster = repos
            .cluster_membership
            .insert_with_members(&NewCluster {
                txids: txids.iter().map(|t| t.to_string()).collect(),
                total_weight: txids.len() as i64 * TX_WEIGHT,
                total_fee: txids.len() as i64 * TX_FEE,
                first_seen_at: fixed_time(),
            })
            .await
            .expect("insert");
        ids.push(cluster.id);
    }
    let (abc, de) = (ids[0], ids[1]);

    let mined_abc = vec!["a".to_string()];
    let mined_de = vec!["e".to_string()];
    let newly_confirmed = repos
        .cluster_membership
        .trim_and_confirm_many(
            &[
                ClusterMembershipUpdate {
                    cluster_id: de,
                    current_members: &mined_de,
                    total_weight: TX_WEIGHT,
                    total_fee: 70,
                },
                ClusterMembershipUpdate {
                    cluster_id: abc,
                    current_members: &mined_abc,
                    total_weight: TX_WEIGHT,
                    total_fee: 50,
                },
            ],
            fixed_time(),
        )
        .await
        .expect("trim_and_confirm_many");
    assert_eq!(newly_confirmed, 2);

    for (id, mined, fee) in [(abc, &mined_abc, 50), (de, &mined_de, 70)] {
        let row = repos
            .cluster
            .find_by_ids(&[id])
            .await
            .expect("load")
            .pop()
            .expect("row");
        assert_eq!(&row.txids, mined);
        assert_eq!(row.total_fee, fee);
        assert_eq!(row.status, ClusterStatus::Confirmed);
        assert_eq!(row.confirmed_at, Some(fixed_time()));
    }

    // mined members stay linked to their confirmed cluster, pending ones are unlinked
    assert_eq!(cluster_link(&pool, "a").await, Some(abc));
    assert_eq!(cluster_link(&pool, "e").await, Some(de));
    for txid in ["b", "c", "d"] {
        assert_eq!(cluster_link(&pool, txid).await, None, "{txid} still linked");
    }

    // per cluster: opening row, pending members leaving, then the closing row
    let rows = delta_rows(&pool).await;
    for (id, pending, mined) in [(abc, vec!["b", "c"], "a"), (de, vec!["d"], "e")] {
        let own: Vec<&ClusterDelta> = rows.iter().filter(|r| r.cluster_id == id).collect();
        assert_eq!(own.len(), 3, "cluster {id}");
        assert_eq!(sorted(&own[1].removed_txids), pending);
        assert_eq!(own[2].removed_txids, vec![mined.to_string()]);
        assert_delta_zero(&rows, id);
    }
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
    let weights = HashMap::from([("a".to_string(), TX_WEIGHT), ("b".to_string(), TX_WEIGHT)]);
    svc.confirm_mined(&["a".into(), "b".into()], &fees, &weights, fixed_time())
        .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 4);

    // still-pending members leave first, with the confirmed-subset totals
    assert_eq!(rows[1].cluster_id, original.id);
    assert_eq!(sorted(&rows[1].removed_txids), vec!["c", "d"]);
    assert_eq!(rows[1].fee_delta, -800);
    assert_eq!(rows[1].weight_delta, -2 * TX_WEIGHT);

    // then the confirm closes the original cluster
    assert_eq!(rows[2].cluster_id, original.id);
    assert_eq!(sorted(&rows[2].removed_txids), vec!["a", "b"]);
    assert_eq!(rows[2].fee_delta, -1000);
    assert_eq!(rows[2].weight_delta, -2 * TX_WEIGHT);
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
    assert_eq!(rows[3].weight_delta, 2 * TX_WEIGHT);
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
    assert_eq!(rows[1].weight_delta, 0);
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
    assert_eq!(shrunk.total_weight, 2 * TX_WEIGHT);

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert_eq!(rows[1].removed_txids, vec!["c"]);
    assert_eq!(rows[1].fee_delta, -TX_FEE);
    assert_eq!(rows[1].weight_delta, -TX_WEIGHT);

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
    assert_eq!(rows[1].weight_delta, -TX_WEIGHT);

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
    assert_eq!(rows[1].weight_delta, -2 * TX_WEIGHT);
    assert_delta_zero(&rows, stored.id);

    let closed = repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(closed.status, ClusterStatus::Evicted);
    assert_eq!(sorted(&closed.txids), vec!["a", "b"]);
    assert_eq!(closed.total_fee, 1000);
    assert_eq!(closed.total_weight, 2 * TX_WEIGHT);
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
    let weights = HashMap::from([("a".to_string(), TX_WEIGHT), ("b".to_string(), TX_WEIGHT)]);
    svc.confirm_mined(&["a".into(), "b".into()], &fees, &weights, fixed_time())
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
    let reconciler = deps.mempool_reconciler();
    deps.mempool_ledger.assert_present(&["c".to_string()]);
    reconciler.tick().await;

    retriever.set_clusters(vec![
        ClusterFixture::new(&["a", "b"])
            .with_total_fee_sats(1000)
            .build(),
    ]);

    let mut frames = Box::pin(svc.get_delta_stream().await);

    deps.mempool_ledger.assert_absent(&["c".to_string()]);
    reconciler.tick().await;

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
