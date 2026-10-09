#![cfg(feature = "db_integration_tests")]

use api::db::models::{ClusterStatus, NewCluster};
use api::db::schema::clusters;
use api::db::{ACTIVE_IDS_JOIN_MIN_TXIDS, ClusterVersionUpdate, DbPool, Repos};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::models::ClusterChunk;
use testkit::deps::{cluster_service, deps};
use testkit::fixtures::{ClusterFixture, TX_FEE, TX_WEIGHT, fixed_time, members, seed_txs};
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
    let mut txids = members(&stored);
    txids.sort();
    assert_eq!(txids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(stored.total_fee, 1500);
    assert_eq!(stored.total_weight, 2 * TX_WEIGHT);

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
        assert_eq!(members(&stored), vec![txid.to_string()]);
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
async fn syncs_only_the_live_txs_no_active_cluster_holds() {
    let pool = isolated_pool().await;
    let retriever =
        MockClusterRetriever::with_clusters(vec![ClusterFixture::new(&["a", "b"]).build()]);
    let deps = deps(pool).with_cluster_retriever(retriever.clone());
    seed_txs(&deps.repos.transaction, &["a", "b", "c"]).await;
    let service = deps.cluster_service();
    service.sync_clusters_for(&["a".into()], &[]).await;
    deps.mempool_ledger
        .seed(["a", "b", "c"].iter().map(|s| s.to_string()).collect());

    service.sync_uncovered_live_txs().await;

    // a's cluster already held a and b, so only c reached the node again
    assert_eq!(
        retriever.clusters_fetched(),
        vec!["a".to_string(), "c".to_string()]
    );
    let stored = deps
        .repos
        .cluster
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("cluster exists");
    assert_eq!(members(&stored), vec!["c".to_string()]);
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
    assert_eq!(updated.total_weight, 3 * TX_WEIGHT);
    assert_eq!(members(&updated).len(), 3);
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
    let mut txids = members(&merged);
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
    let mut loser_txids = members(&loser);
    loser_txids.sort();
    assert_eq!(loser_txids, vec!["c".to_string(), "d".to_string()]);
    assert_eq!(loser.total_fee, 800);
    assert_eq!(loser.total_weight, 2 * TX_WEIGHT);

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
    assert_eq!(merged.total_weight, 4 * TX_WEIGHT);
    assert_eq!(members(&merged).len(), 4);
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
    let mut txids = members(&updated);
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
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["a", "b"]).build_new_cluster())
        .await
        .expect("insert");

    for member in ["a", "b"] {
        for input in lookup_inputs(&[member]) {
            let ids = repos
                .cluster
                .find_active_ids_by_txids(&input)
                .await
                .expect("query");
            assert_eq!(
                ids,
                vec![cluster.id],
                "not found by member {member} among {} txids",
                input.len()
            );
        }
    }
}

#[tokio::test]
async fn finds_active_cluster_once_for_several_matching_members() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let cluster = repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["a", "b", "c"]).build_new_cluster())
        .await
        .expect("insert");

    for input in lookup_inputs(&["a", "b", "c"]) {
        let ids = repos
            .cluster
            .find_active_ids_by_txids(&input)
            .await
            .expect("query");
        assert_eq!(
            ids,
            vec![cluster.id],
            "cluster returned once, not once per matching member, among {} txids",
            input.len()
        );
    }
}

#[tokio::test]
async fn finds_every_active_cluster_the_txids_touch() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let mut expected = Vec::new();
    for txids in [["a", "b"], ["c", "d"]] {
        let cluster = repos
            .cluster_membership
            .insert_with_members(&ClusterFixture::new(&txids).build_new_cluster())
            .await
            .expect("insert");
        expected.push(cluster.id);
    }
    expected.sort();

    for input in lookup_inputs(&["a", "d"]) {
        let mut ids = repos
            .cluster
            .find_active_ids_by_txids(&input)
            .await
            .expect("query");
        ids.sort();
        assert_eq!(
            ids,
            expected,
            "a cluster missed among {} txids",
            input.len()
        );
    }
}

#[tokio::test]
async fn excludes_a_confirmed_cluster_that_still_holds_its_txids() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let cluster = repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["a", "b"]).build_new_cluster())
        .await
        .expect("insert");

    // confirming keeps the current version's chunks on purpose (production
    // still needs them to link confirmed member txs), so its txids look -- to
    // a plain overlap query -- exactly like a live cluster a mined block should
    // confirm
    repos
        .cluster_membership
        .confirm_many(&[cluster.id], fixed_time())
        .await
        .expect("confirm");
    let confirmed = repos
        .cluster
        .find_by_ids(&[cluster.id])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(
        members(&confirmed).len(),
        2,
        "test setup invalid: confirm must keep the chunks"
    );

    for input in lookup_inputs(&["a"]) {
        let ids = repos
            .cluster
            .find_active_ids_by_txids(&input)
            .await
            .expect("query");
        assert!(
            ids.is_empty(),
            "a mined block reopened an already-confirmed cluster, among {} txids",
            input.len()
        );
    }
}

#[tokio::test]
async fn excludes_a_closed_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let cluster = repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["a"]).build_new_cluster())
        .await
        .expect("insert");
    repos
        .cluster_membership
        .mark_evicted(&[cluster.id])
        .await
        .expect("close");

    for input in lookup_inputs(&["a"]) {
        let ids = repos
            .cluster
            .find_active_ids_by_txids(&input)
            .await
            .expect("query");
        assert!(
            ids.is_empty(),
            "closed cluster returned by an active lookup among {} txids",
            input.len()
        );
    }
}

#[tokio::test]
async fn find_by_txid_prefers_the_active_cluster_over_a_closed_one() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let evicted = repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["a"]).build_new_cluster())
        .await
        .expect("insert");
    repos
        .cluster_membership
        .mark_evicted(&[evicted.id])
        .await
        .expect("close");
    // a comes back to the mempool and gets a cluster of its own
    let current = repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["a"]).build_new_cluster())
        .await
        .expect("insert");

    let found = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(found.id, current.id);
}

#[tokio::test]
async fn find_by_txid_prefers_the_cluster_that_closed_last() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let keeper = repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["a"]).build_new_cluster())
        .await
        .expect("insert");
    let loser = repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["b"]).build_new_cluster())
        .await
        .expect("insert");
    repos
        .cluster_membership
        .mark_merged(&[loser.id])
        .await
        .expect("merge");
    repos
        .cluster_membership
        .replace_chunks(ClusterVersionUpdate {
            cluster_id: keeper.id,
            chunks: &[chunk(&["a"]), chunk(&["b"])],
        })
        .await
        .expect("absorb");
    repos
        .cluster_membership
        .confirm_many(&[keeper.id], fixed_time())
        .await
        .expect("confirm");
    // pinned, so the order does not hang on the local clock
    set_closed_at(&pool, loser.id, fixed_time() - time::Duration::minutes(1)).await;
    set_closed_at(&pool, keeper.id, fixed_time()).await;

    let found = repos
        .cluster
        .find_by_txid("b")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(
        found.id, keeper.id,
        "b was mined in the keeper, not in the cluster merged into it"
    );
}

async fn set_closed_at(pool: &DbPool, cluster_id: i64, closed_at: OffsetDateTime) {
    let mut conn = pool.get().await.expect("conn");
    diesel::update(clusters::table.find(cluster_id))
        .set(clusters::closed_at.eq(closed_at))
        .execute(&mut conn)
        .await
        .expect("set closed_at");
}

#[tokio::test]
async fn finds_a_member_of_any_chunk() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let cluster = repos
        .cluster_membership
        .insert_with_members(&NewCluster {
            chunks: vec![chunk(&["a"]), chunk(&["b", "c"])],
            first_seen_at: fixed_time(),
        })
        .await
        .expect("insert");

    for input in lookup_inputs(&["c"]) {
        let ids = repos
            .cluster
            .find_active_ids_by_txids(&input)
            .await
            .expect("query");
        assert_eq!(
            ids,
            vec![cluster.id],
            "member of the second chunk missed among {} txids",
            input.len()
        );
    }
}

#[tokio::test]
async fn excludes_a_member_only_a_past_version_held() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool);
    let cluster = repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["a", "b"]).build_new_cluster())
        .await
        .expect("insert");
    repos
        .cluster_membership
        .replace_chunks(ClusterVersionUpdate {
            cluster_id: cluster.id,
            chunks: &[chunk(&["a"])],
        })
        .await
        .expect("new version");

    for input in lookup_inputs(&["b"]) {
        let ids = repos
            .cluster
            .find_active_ids_by_txids(&input)
            .await
            .expect("query");
        assert!(
            ids.is_empty(),
            "a member only version 1 held was found among {} txids",
            input.len()
        );
    }
    for input in lookup_inputs(&["a"]) {
        let ids = repos
            .cluster
            .find_active_ids_by_txids(&input)
            .await
            .expect("query");
        assert_eq!(ids, vec![cluster.id], "among {} txids", input.len());
    }
}

fn chunk(txids: &[&str]) -> ClusterChunk {
    ClusterChunk {
        txids: txids.iter().map(|t| t.to_string()).collect(),
        fee_sats: txids.len() as u64 * TX_FEE as u64,
        weight: txids.len() as u64 * TX_WEIGHT as u64,
    }
}

/// The lookup as given, and padded with misses past the size where the
/// repository switches to its join form, so each case covers both queries.
fn lookup_inputs(txids: &[&str]) -> [Vec<String>; 2] {
    let given: Vec<String> = txids.iter().map(|t| t.to_string()).collect();
    let mut padded = given.clone();
    padded.extend((0..ACTIVE_IDS_JOIN_MIN_TXIDS).map(|i| format!("miss-{i}")));
    [given, padded]
}
