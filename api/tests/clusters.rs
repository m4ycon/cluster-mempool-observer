#![cfg(feature = "db_integration_tests")]

use api::db::Repos;
use api::db::models::{ClusterStatus, NewCluster};
use testkit::deps::{cluster_service, deps};
use testkit::fixtures::{ClusterFixture, TX_VSIZE, fixed_time, seed_txs};
use testkit::mocks::MockClusterRetriever;
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

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
async fn published_cluster_carries_the_stored_first_seen_at() {
    let pool = isolated_pool().await;
    let retriever =
        MockClusterRetriever::with_clusters(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool).with_cluster_retriever(retriever);
    seed_txs(&deps.repos.transaction, &["a", "b"]).await;

    let service = deps.cluster_service();
    let before = OffsetDateTime::now_utc();
    service
        .sync_clusters_for(&["a".into(), "b".into()], &[])
        .await;

    let stored = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");
    assert!(
        stored.first_seen_at >= before,
        "insert stamps our clock, not the node's time"
    );

    let published = service.get_current_snapshot();
    assert_eq!(published.len(), 1, "one frame is enough for one cluster");
    assert_eq!(published[0].upserted.len(), 1);
    assert_eq!(published[0].upserted[0].first_seen_at, stored.first_seen_at);
}

#[tokio::test]
async fn persists_singleton_clusters() {
    let pool = isolated_pool().await;
    let deps = deps(pool).with_cluster_retriever(MockClusterRetriever::with_clusters(vec![]));
    seed_txs(&deps.repos.transaction, &["c", "d"]).await;

    // default mock reports every tx as a cluster of its own, like the node does
    deps.cluster_service()
        .sync_clusters_for(&["c".into(), "d".into()], &[])
        .await;

    // both went in through the batched insert path
    assert_eq!(deps.repos.cluster.count().await.expect("count"), 2);
    for txid in ["c", "d"] {
        let stored = deps
            .repos
            .cluster
            .find_by_txid(txid)
            .await
            .expect("query")
            .expect("cluster exists");
        assert_eq!(stored.txids, vec![txid.to_string()]);
        assert_eq!(
            deps.repos
                .transaction
                .get_cluster_ids_by_txids(&[txid.into()])
                .await
                .expect("ids"),
            vec![stored.id],
            "{txid} is not linked to its own cluster"
        );
    }
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
async fn member_leaving_a_cluster_gets_one_of_its_own() {
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

    // c left {a,b} but is still in the mempool, so it must end up in a cluster
    // of its own rather than orphaned with a stale cluster_id
    let c_links = repos
        .transaction
        .get_cluster_ids_by_txids(&["c".into()])
        .await
        .expect("ids");
    assert_eq!(c_links.len(), 1, "c belongs to exactly one cluster");
    assert_ne!(
        c_links[0], initial.id,
        "c still linked to the cluster it left"
    );

    let c_cluster = repos
        .cluster
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(c_cluster.id, c_links[0]);
    assert_eq!(c_cluster.txids, vec!["c".to_string()]);
    assert_eq!(repos.cluster.find_active().await.expect("active").len(), 2);
}

#[tokio::test]
async fn merging_two_singletons_keeps_the_older_first_seen_at() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_txs(&repos.transaction, &["a", "b"]).await;

    // a and b reach the mempool alone, a first
    cluster_service(pool.clone(), vec![ClusterFixture::new(&["a"]).build()])
        .sync_clusters_for(&["a".into()], &[])
        .await;
    let a_alone = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    cluster_service(pool.clone(), vec![ClusterFixture::new(&["b"]).build()])
        .sync_clusters_for(&["b".into()], &[])
        .await;
    let b_alone = repos
        .cluster
        .find_by_txid("b")
        .await
        .expect("query")
        .expect("exists");
    assert!(a_alone.first_seen_at < b_alone.first_seen_at);

    // b turns out to spend from a: the node now reports one group
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

    let merged = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(
        merged.id, a_alone.id,
        "the older row is the one that survives"
    );
    let mut txids = merged.txids.clone();
    txids.sort();
    assert_eq!(txids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(
        merged.first_seen_at, a_alone.first_seen_at,
        "the group is as old as its oldest member"
    );
    assert_eq!(repos.cluster.find_active().await.expect("active").len(), 1);
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

    // the loser row survives but is marked merged: membership and totals
    // stay exactly as they were the moment it lost
    assert_eq!(repos.cluster.count().await.expect("count"), 2);
    let loser = repos
        .cluster
        .find_by_ids(&[cd.id])
        .await
        .expect("query")
        .pop()
        .expect("loser row kept");
    assert_eq!(loser.status, ClusterStatus::Merged);
    let mut loser_txids = loser.txids.clone();
    loser_txids.sort();
    assert_eq!(loser_txids, vec!["c".to_string(), "d".to_string()]);
    assert_eq!(loser.total_fee, 800);
    assert_eq!(loser.total_vsize, 2 * TX_VSIZE);

    let active = repos.cluster.find_active().await.expect("active");
    assert_eq!(active.len(), 1, "only the keeper stays active");

    let holding_d = repos
        .cluster
        .find_active_ids_by_txids(&["d".into()])
        .await
        .expect("query");
    assert_eq!(
        holding_d,
        vec![ab.id],
        "the merged loser still lists d, so only the status tells the keeper from it"
    );
    let merged = repos
        .cluster
        .find_by_ids(&holding_d)
        .await
        .expect("query")
        .pop()
        .expect("keeper row kept");
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

    // detach: the dropped tx no longer back-links to this cluster (it is still
    // in the mempool, so it gets a cluster of its own instead)
    let dropped_links = repos
        .transaction
        .get_cluster_ids_by_txids(&["dropped".into()])
        .await
        .expect("ids");
    assert_ne!(
        dropped_links,
        vec![initial.id],
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

#[tokio::test]
async fn finds_active_cluster_by_any_single_member() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let cluster = repos
        .cluster
        .insert(&NewCluster {
            txids: vec!["a".into(), "b".into()],
            total_vsize: 2 * TX_VSIZE,
            total_fee: 1000,
            first_seen_at: fixed_time(),
        })
        .await
        .expect("insert");

    for member in ["a", "b"] {
        let ids = repos
            .cluster
            .find_active_ids_by_txids(&[member.to_string()])
            .await
            .expect("query");
        assert_eq!(ids, vec![cluster.id], "not found by member {member}");
    }
}

#[tokio::test]
async fn finds_active_cluster_once_for_several_matching_members() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let cluster = repos
        .cluster
        .insert(&NewCluster {
            txids: vec!["a".into(), "b".into(), "c".into()],
            total_vsize: 3 * TX_VSIZE,
            total_fee: 1500,
            first_seen_at: fixed_time(),
        })
        .await
        .expect("insert");

    let ids = repos
        .cluster
        .find_active_ids_by_txids(&["a".into(), "b".into(), "c".into()])
        .await
        .expect("query");
    assert_eq!(
        ids,
        vec![cluster.id],
        "cluster returned once, not once per matching member"
    );
}

#[tokio::test]
async fn excludes_a_confirmed_cluster_that_still_holds_its_txids() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let cluster = repos
        .cluster
        .insert(&NewCluster {
            txids: vec!["a".into(), "b".into()],
            total_vsize: 2 * TX_VSIZE,
            total_fee: 1000,
            first_seen_at: fixed_time(),
        })
        .await
        .expect("insert");

    // confirm() keeps the row's txids on purpose (production still needs them
    // to link confirmed member txs), so the row looks -- to a plain overlap
    // query -- exactly like a live cluster that a mined block should confirm
    let confirmed = repos
        .cluster_membership
        .confirm(cluster.id, fixed_time())
        .await
        .expect("confirm");
    assert_eq!(
        confirmed.txids.len(),
        2,
        "test setup invalid: confirm must keep the txids"
    );

    let ids = repos
        .cluster
        .find_active_ids_by_txids(&["a".into()])
        .await
        .expect("query");
    assert!(
        ids.is_empty(),
        "a mined block reopened an already-confirmed cluster"
    );
}

#[tokio::test]
async fn excludes_a_closed_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    repos
        .cluster
        .insert(&NewCluster {
            txids: vec![],
            total_vsize: 0,
            total_fee: 0,
            first_seen_at: fixed_time(),
        })
        .await
        .expect("insert");

    let ids = repos
        .cluster
        .find_active_ids_by_txids(&["a".into()])
        .await
        .expect("query");
    assert!(
        ids.is_empty(),
        "closed cluster returned by an active lookup"
    );
}
