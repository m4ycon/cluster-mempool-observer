#![cfg(feature = "db_integration_tests")]

use api::db::Repos;
use testkit::deps::{cluster_service, deps};
use testkit::fixtures::{ClusterFixture, TX_VSIZE, seed_txs};
use testkit::mocks::MockClusterRetriever;
use testkit::postgres::isolated_pool;

#[tokio::test]
async fn stores_multi_tx_cluster_and_links_member_txs() {
    let pool = isolated_pool().await;
    let retriever = MockClusterRetriever::with_clusters(vec![
        ClusterFixture::new(&["a", "b"])
            .with_total_fee_sats(1500)
            .build(),
    ]);
    let deps = deps(pool).with_cluster_retriever(retriever.clone());
    seed_txs(&deps.repos.transaction, &["a", "b"]).await;

    deps.cluster_service()
        .sync_clusters_for(&["a".into(), "b".into()], &[])
        .await;

    // one cluster fetch only, b is covered by a's cluster (dedup)
    assert_eq!(retriever.clusters_fetched(), vec!["a".to_string()]);

    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");
    let mut txids = stored.txids.clone();
    txids.sort();
    assert_eq!(txids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(stored.total_fee, 1500);
    assert_eq!(stored.total_vsize, 2 * TX_VSIZE);

    // both member txs link to the cluster
    let ids = deps
        .repos
        .transaction
        .get_cluster_ids_by_txids(&["a".into(), "b".into()])
        .await
        .expect("ids");
    assert_eq!(ids, vec![stored.id]);
}

#[tokio::test]
async fn skips_singleton_clusters() {
    let pool = isolated_pool().await;
    let deps = deps(pool).with_cluster_retriever(MockClusterRetriever::with_clusters(vec![]));
    seed_txs(&deps.repos.transaction, &["c"]).await;

    // default mock reports every tx as a singleton
    deps.cluster_service()
        .sync_clusters_for(&["c".into()], &[])
        .await;

    assert_eq!(deps.repos.cluster.count().await.expect("count"), 0);
    assert!(
        deps.repos
            .transaction
            .get_cluster_ids_by_txids(&["c".into()])
            .await
            .expect("ids")
            .is_empty()
    );
}

#[tokio::test]
async fn updates_existing_cluster_when_group_grows() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_txs(&repos.transaction, &["a", "b", "c"]).await;

    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1000)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into(), "b".into()], &[])
    .await;
    let first = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    // c joins the cluster
    cluster_service(
        pool,
        vec![
            ClusterFixture::new(&["a", "b", "c"])
                .with_total_fee_sats(1500)
                .build(),
        ],
    )
    .sync_clusters_for(&["c".into()], &[])
    .await;

    assert_eq!(repos.cluster.count().await.expect("count"), 1);
    let updated = repos
        .cluster
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(updated.id, first.id); // same row, updated in place
    assert_eq!(updated.total_fee, 1500);
    assert_eq!(updated.total_vsize, 3 * TX_VSIZE);
    assert_eq!(updated.txids.len(), 3);
}

#[tokio::test]
async fn clears_orphan_cluster_id_when_member_leaves_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_txs(&repos.transaction, &["a", "b", "c"]).await;

    // initial mempool cluster {a,b,c}; all three member txs get linked
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
    let initial = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(initial.txids.len(), 3);
    assert_eq!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&["c".into()])
            .await
            .expect("ids"),
        vec![initial.id]
    );

    // c drops out of the mempool cluster, and the retriever now reports {a,b}
    cluster_service(
        pool,
        vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1000)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;

    // cluster row correctly reports {a,b}, same id
    let shrunk = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    let mut txids = shrunk.txids.clone();
    txids.sort();
    assert_eq!(txids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(shrunk.id, initial.id);

    // c no longer belongs to any cluster, so its back-reference must be cleared
    let c_links = repos
        .transaction
        .get_cluster_ids_by_txids(&["c".into()])
        .await
        .expect("ids");
    assert!(
        c_links.is_empty(),
        "c still linked to cluster {c_links:?} after leaving it (stale cluster_id)"
    );
}

#[tokio::test]
async fn merges_clusters_into_one_row() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_txs(&repos.transaction, &["a", "b", "c", "d"]).await;

    // two separate clusters first
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
    assert_eq!(repos.cluster.count().await.expect("count"), 2);

    // {a,b} was observed before {c,d}, so it carries the older first_seen_at
    let ab = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    let cd = repos
        .cluster
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    let oldest_first_seen = ab.first_seen_at;
    assert!(oldest_first_seen < cd.first_seen_at);

    // a linking tx merges them
    cluster_service(
        pool,
        vec![
            ClusterFixture::new(&["a", "b", "c", "d"])
                .with_total_fee_sats(1800)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;

    // the loser row survives but is closed: empty members, zeroed totals
    assert_eq!(repos.cluster.count().await.expect("count"), 2);
    let loser = repos
        .cluster
        .find_by_ids(&[cd.id])
        .await
        .expect("query")
        .pop()
        .expect("loser row kept");
    assert!(loser.txids.is_empty());
    assert_eq!(loser.total_fee, 0);
    assert_eq!(loser.total_vsize, 0);

    let active = repos.cluster.find_active().await.expect("active");
    assert_eq!(active.len(), 1, "only the keeper stays active");

    let merged = repos
        .cluster
        .find_by_txid("d")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(merged.total_fee, 1800);
    assert_eq!(merged.total_vsize, 4 * TX_VSIZE);
    assert_eq!(merged.txids.len(), 4);
    assert_eq!(merged.first_seen_at, oldest_first_seen);

    // all four txs point at the single surviving cluster
    let ids = repos
        .transaction
        .get_cluster_ids_by_txids(&["a".into(), "b".into(), "c".into(), "d".into()])
        .await
        .expect("ids");
    assert_eq!(ids, vec![merged.id]);
}

#[tokio::test]
async fn upsert_detaches_dropped_member_and_links_new_member() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    // dropped: member of the first cluster, leaves on the second sync
    // a, b: stay members across both syncs
    // new: not part of the first cluster, joins on the second sync
    seed_txs(&repos.transaction, &["dropped", "a", "b", "new"]).await;

    // initial mempool cluster {dropped, a, b}; all three get linked
    cluster_service(
        pool.clone(),
        vec![
            ClusterFixture::new(&["dropped", "a", "b"])
                .with_total_fee_sats(1500)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;
    let initial = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&["dropped".into()])
            .await
            .expect("ids"),
        vec![initial.id]
    );

    // mempool now reports {a, b, new}: `dropped` leaves and `new` joins
    cluster_service(
        pool,
        vec![
            ClusterFixture::new(&["a", "b", "new"])
                .with_total_fee_sats(1800)
                .build(),
        ],
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;

    // cluster row, same id, holds the new member set
    let updated = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(updated.id, initial.id);
    let mut txids = updated.txids.clone();
    txids.sort();
    assert_eq!(
        txids,
        vec!["a".to_string(), "b".to_string(), "new".to_string()]
    );

    // detach: the dropped tx no longer back-links to the cluster
    assert!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&["dropped".into()])
            .await
            .expect("ids")
            .is_empty(),
        "dropped tx still linked after leaving the cluster"
    );

    // link: the newly added member back-links to the cluster
    assert_eq!(
        repos
            .transaction
            .get_cluster_ids_by_txids(&["new".into()])
            .await
            .expect("ids"),
        vec![initial.id],
        "newly added member was not linked to the cluster"
    );
}
