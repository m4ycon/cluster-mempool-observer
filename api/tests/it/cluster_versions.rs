#![cfg(feature = "db_integration_tests")]

use api::db::models::{ClusterChunkRow, ClusterStatus, NewCluster};
use api::db::schema::{cluster_chunks, transactions};
use api::db::{ClusterVersionUpdate, DbPool, Repos, TransactionRepository};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use futures::StreamExt;
use shared::models::{ClusterChunk, GetMempoolClusterModel};
use std::collections::{BTreeSet, HashMap};
use std::time::Duration;
use testkit::deps::{cluster_service, deps, strict_cluster_service};
use testkit::fixtures::{ClusterFixture, TX_FEE, TX_WEIGHT, fixed_time, members, seed_sized_txs};
use testkit::mocks::{MockClusterRetriever, MockTransactionRetriever};
use testkit::postgres::isolated_pool;

async fn chunk_rows(pool: &DbPool) -> Vec<ClusterChunkRow> {
    let mut conn = pool.get().await.expect("conn");
    cluster_chunks::table
        .order((
            cluster_chunks::cluster_id.asc(),
            cluster_chunks::version.asc(),
            cluster_chunks::position.asc(),
        ))
        .select(ClusterChunkRow::as_select())
        .load(&mut conn)
        .await
        .expect("load cluster chunks")
}

/// Every version of the cluster, oldest first, as its chunks' txids.
fn versions(rows: &[ClusterChunkRow], cluster_id: i64) -> Vec<Vec<Vec<String>>> {
    let mut out: Vec<Vec<Vec<String>>> = Vec::new();
    let mut current = None;
    for row in rows.iter().filter(|r| r.cluster_id == cluster_id) {
        if current != Some(row.version) {
            assert_eq!(
                row.version,
                out.len() as i32 + 1,
                "versions of cluster {cluster_id} must count up from 1"
            );
            out.push(Vec::new());
            current = Some(row.version);
        }
        out.last_mut().expect("pushed").push(row.txids.clone());
    }
    out
}

fn txids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

fn sorted(txids: &[String]) -> Vec<String> {
    let mut v = txids.to_vec();
    v.sort();
    v
}

fn chunk(ids: &[&str], fee_sats: u64, weight: u64) -> ClusterChunk {
    ClusterChunk {
        txids: txids(ids),
        fee_sats,
        weight,
    }
}

fn node_cluster(chunks: Vec<ClusterChunk>) -> GetMempoolClusterModel {
    GetMempoolClusterModel {
        cluster_weight: chunks.iter().map(|chunk| chunk.weight).sum(),
        tx_count: chunks.iter().map(|chunk| chunk.txids.len() as u32).sum(),
        chunks,
    }
}

/// The by-txid lookup trusts `live`, so it must mark exactly the current
/// version of each active cluster and nothing else.
async fn assert_live_matches_state(repos: &Repos, pool: &DbPool) {
    let rows = chunk_rows(pool).await;
    let ids: Vec<i64> = rows
        .iter()
        .map(|r| r.cluster_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for cluster in repos.cluster.find_by_ids(&ids).await.expect("load") {
        let live: Vec<&ClusterChunkRow> = rows
            .iter()
            .filter(|r| r.cluster_id == cluster.id && r.live)
            .collect();
        if cluster.status == ClusterStatus::Active {
            assert_eq!(live.len(), cluster.chunks.len(), "cluster {}", cluster.id);
            assert!(
                live.iter().all(|r| r.version == cluster.version),
                "cluster {} has a live chunk off its current version",
                cluster.id
            );
        } else {
            assert!(
                live.is_empty(),
                "closed cluster {} kept live chunks",
                cluster.id
            );
        }
    }
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
async fn new_cluster_is_version_1_with_live_chunks() {
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
    assert_eq!(stored.version, 1);
    assert_eq!(stored.closed_at, None);

    let rows = chunk_rows(&pool).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cluster_id, stored.id);
    assert_eq!(rows[0].position, 0);
    assert_eq!(rows[0].txids, txids(&["a", "b"]));
    assert_eq!(rows[0].fee, 1500);
    assert_eq!(rows[0].weight, 2 * TX_WEIGHT);
    assert!(rows[0].live);
    assert_live_matches_state(&deps.repos, &pool).await;
}

#[tokio::test]
async fn insert_keeps_chunks_in_mining_order_and_sums_them_into_the_totals() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());

    let cluster = repos
        .cluster_membership
        .insert_with_members(&NewCluster {
            chunks: vec![chunk(&["p", "c1"], 900, 1200), chunk(&["c2"], 100, 600)],
            first_seen_at: fixed_time(),
        })
        .await
        .expect("insert");

    assert_eq!(cluster.total_fee, 1000);
    assert_eq!(cluster.total_weight, 1800);
    let loaded = repos
        .cluster
        .find_by_ids(&[cluster.id])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(
        loaded.chunks,
        vec![chunk(&["p", "c1"], 900, 1200), chunk(&["c2"], 100, 600)]
    );
    assert_eq!(members(&loaded), txids(&["p", "c1", "c2"]));
    let positions: Vec<i16> = chunk_rows(&pool).await.iter().map(|r| r.position).collect();
    assert_eq!(positions, vec![0, 1]);
}

#[tokio::test]
async fn batched_insert_gives_each_cluster_its_own_chunks() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let batch: Vec<NewCluster> = ["x", "y", "z"]
        .iter()
        .map(|txid| ClusterFixture::new(&[txid]).build_new_cluster())
        .collect();

    let inserted = repos
        .cluster_membership
        .insert_many_with_members(&batch)
        .await
        .expect("insert");

    assert_eq!(inserted.len(), 3);
    let rows = chunk_rows(&pool).await;
    for cluster in &inserted {
        let txid = members(cluster).pop().expect("one member");
        assert_eq!(versions(&rows, cluster.id), vec![vec![vec![txid.clone()]]]);
        assert_eq!(cluster_link(&pool, &txid).await, Some(cluster.id));
    }
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn growth_writes_a_new_version_and_retires_the_old_one() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b", "c"]).await;

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

    let stored = repos
        .cluster
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(stored.version, 2);
    assert_eq!(stored.total_fee, 1500);
    assert_eq!(stored.total_weight, 3 * TX_WEIGHT);
    assert_eq!(
        versions(&chunk_rows(&pool).await, stored.id),
        vec![vec![txids(&["a", "b"])], vec![txids(&["a", "b", "c"])]]
    );
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn departure_writes_a_new_version_without_the_member() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b", "c"]).await;

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

    let stored = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(
        versions(&chunk_rows(&pool).await, stored.id),
        vec![vec![txids(&["a", "b", "c"])], vec![txids(&["a", "b"])]]
    );
    assert_eq!(stored.total_fee, 1000);
    assert_eq!(cluster_link(&pool, "c").await, None);
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn unchanged_resync_writes_nothing() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    seed_sized_txs(&tx_repo, &["a", "b"]).await;

    for candidate in ["a", "b"] {
        cluster_service(
            pool.clone(),
            vec![
                ClusterFixture::new(&["a", "b"])
                    .with_total_fee_sats(1500)
                    .build(),
            ],
        )
        .sync_clusters_for(&[candidate.into()], &[])
        .await;
    }

    let rows = chunk_rows(&pool).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].version, 1);
}

#[tokio::test]
async fn order_only_change_writes_a_new_version() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let cluster = repos
        .cluster_membership
        .insert_with_members(&NewCluster {
            chunks: vec![chunk(&["a"], 500, 400), chunk(&["b"], 300, 400)],
            first_seen_at: fixed_time(),
        })
        .await
        .expect("insert");

    let updated = repos
        .cluster_membership
        .replace_chunks(ClusterVersionUpdate {
            cluster_id: cluster.id,
            chunks: &[chunk(&["b"], 300, 400), chunk(&["a"], 500, 400)],
        })
        .await
        .expect("replace");

    assert_eq!(updated.version, 2);
    assert_eq!(
        (updated.total_fee, updated.total_weight),
        (cluster.total_fee, cluster.total_weight)
    );
    assert_eq!(
        versions(&chunk_rows(&pool).await, cluster.id),
        vec![
            vec![txids(&["a"]), txids(&["b"])],
            vec![txids(&["b"]), txids(&["a"])]
        ]
    );
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn order_only_resync_writes_a_new_version() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b"]).await;

    for [first, second] in [["a", "b"], ["b", "a"]] {
        cluster_service(
            pool.clone(),
            vec![node_cluster(vec![
                chunk(&[first], 500, 400),
                chunk(&[second], 500, 400),
            ])],
        )
        .sync_clusters_for(&["a".into()], &[])
        .await;
    }

    let cluster = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(
        versions(&chunk_rows(&pool).await, cluster.id),
        vec![
            vec![txids(&["a"]), txids(&["b"])],
            vec![txids(&["b"]), txids(&["a"])]
        ]
    );
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn fee_only_change_writes_a_new_version() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b"]).await;

    for fee in [1000, 1500] {
        cluster_service(
            pool.clone(),
            vec![
                ClusterFixture::new(&["a", "b"])
                    .with_total_fee_sats(fee)
                    .build(),
            ],
        )
        .sync_clusters_for(&["a".into()], &[])
        .await;
    }

    let rows = chunk_rows(&pool).await;
    let fees: Vec<(i32, i64, bool)> = rows.iter().map(|r| (r.version, r.fee, r.live)).collect();
    assert_eq!(fees, vec![(1, 1000, false), (2, 1500, true)]);
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn merge_closes_the_loser_without_a_new_version() {
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

    let rows = chunk_rows(&pool).await;
    assert_eq!(versions(&rows, loser.id), vec![vec![txids(&["c", "d"])]]);
    assert_eq!(
        versions(&rows, keeper.id),
        vec![vec![txids(&["a", "b"])], vec![txids(&["a", "b", "c", "d"])]]
    );
    let closed = repos
        .cluster
        .find_by_ids(&[loser.id])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(closed.status, ClusterStatus::Merged);
    assert_eq!(closed.version, 1);
    assert!(
        closed.closed_at.is_some(),
        "a merged cluster records when it closed"
    );
    assert_eq!(closed.confirmed_at, None);
    let active = repos.cluster.find_active().await.expect("active");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, keeper.id);
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn full_confirm_closes_without_a_new_version() {
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
    let mined: Vec<String> = txids(&["a", "b"]);
    svc.confirm_mined(&mined, &fees, &weights, fixed_time())
        .await;

    let confirmed = repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(confirmed.status, ClusterStatus::Confirmed);
    assert_eq!(confirmed.confirmed_at, Some(fixed_time()));
    assert!(confirmed.closed_at.is_some());
    assert_eq!(confirmed.version, 1);
    assert_eq!(
        members(&confirmed),
        mined,
        "the confirmed cluster keeps its chunks"
    );
    assert_live_matches_state(&repos, &pool).await;

    // re-confirming changes nothing
    let rows_before = chunk_rows(&pool).await.len();
    svc.confirm_mined(&mined, &fees, &weights, fixed_time())
        .await;
    assert_eq!(chunk_rows(&pool).await.len(), rows_before);
    let again = repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(again.closed_at, confirmed.closed_at);

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

async fn insert_cluster(repos: &Repos, ids: &[&str]) -> i64 {
    repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(ids).build_new_cluster())
        .await
        .expect("insert")
        .id
}

#[tokio::test]
async fn confirm_many_closes_each_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let ab = insert_cluster(&repos, &["a", "b"]).await;
    let c = insert_cluster(&repos, &["c"]).await;
    let empty = insert_cluster(&repos, &[]).await;

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
        assert!(row.closed_at.is_some(), "cluster {}", row.id);
        assert_eq!(row.version, 1, "cluster {}", row.id);
    }
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn confirm_many_leaves_an_already_confirmed_cluster_alone() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let ab = insert_cluster(&repos, &["a", "b"]).await;
    let c = insert_cluster(&repos, &["c"]).await;
    let earlier = fixed_time() - time::Duration::minutes(10);
    repos
        .cluster_membership
        .confirm_many(&[ab], earlier)
        .await
        .expect("confirm");
    let first = repos
        .cluster
        .find_by_ids(&[ab])
        .await
        .expect("load")
        .pop()
        .expect("row");

    let confirmed = repos
        .cluster_membership
        .confirm_many(&[ab, c], fixed_time())
        .await
        .expect("confirm_many");
    assert_eq!(confirmed, 1);

    let ab_row = repos
        .cluster
        .find_by_ids(&[ab])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(
        (ab_row.confirmed_at, ab_row.closed_at),
        (Some(earlier), first.closed_at),
        "first confirmation overwritten"
    );
}

#[tokio::test]
async fn trim_and_confirm_many_closes_a_version_of_only_the_given_chunks() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let abc = insert_cluster(&repos, &["a", "b", "c"]).await;
    let de = insert_cluster(&repos, &["d", "e"]).await;

    let mined_abc = vec![chunk(&["a"], 50, TX_WEIGHT as u64)];
    let mined_de = vec![chunk(&["e"], 70, TX_WEIGHT as u64)];
    let newly_confirmed = repos
        .cluster_membership
        .trim_and_confirm_many(
            &[
                ClusterVersionUpdate {
                    cluster_id: de,
                    chunks: &mined_de,
                },
                ClusterVersionUpdate {
                    cluster_id: abc,
                    chunks: &mined_abc,
                },
            ],
            fixed_time(),
        )
        .await
        .expect("trim_and_confirm_many");
    assert_eq!(newly_confirmed, 2);

    let rows = chunk_rows(&pool).await;
    for (id, before, mined, fee) in [
        (abc, txids(&["a", "b", "c"]), "a", 50),
        (de, txids(&["d", "e"]), "e", 70),
    ] {
        let row = repos
            .cluster
            .find_by_ids(&[id])
            .await
            .expect("load")
            .pop()
            .expect("row");
        assert_eq!(row.version, 2);
        assert_eq!(members(&row), txids(&[mined]));
        assert_eq!(row.total_fee, fee);
        assert_eq!(row.status, ClusterStatus::Confirmed);
        assert_eq!(row.confirmed_at, Some(fixed_time()));
        assert_eq!(
            versions(&rows, id),
            vec![vec![before], vec![txids(&[mined])]]
        );
    }

    // mined members stay linked to their confirmed cluster, pending ones are unlinked
    assert_eq!(cluster_link(&pool, "a").await, Some(abc));
    assert_eq!(cluster_link(&pool, "e").await, Some(de));
    for txid in ["b", "c", "d"] {
        assert_eq!(cluster_link(&pool, txid).await, None, "{txid} still linked");
    }
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn replacing_the_chunks_of_a_closed_cluster_changes_nothing() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let ab = insert_cluster(&repos, &["a", "b"]).await;
    repos
        .cluster_membership
        .mark_evicted(&[ab])
        .await
        .expect("close");

    let returned = repos
        .cluster_membership
        .replace_chunks(ClusterVersionUpdate {
            cluster_id: ab,
            chunks: &[chunk(&["a"], 500, 400)],
        })
        .await
        .expect("replace");

    assert_eq!(returned.status, ClusterStatus::Evicted);
    assert_eq!(
        versions(&chunk_rows(&pool).await, ab),
        vec![vec![txids(&["a", "b"])]]
    );
    for txid in ["a", "b"] {
        assert_eq!(
            cluster_link(&pool, txid).await,
            None,
            "{txid} relinked to a closed cluster"
        );
    }
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn confirming_never_relabels_an_evicted_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let ab = insert_cluster(&repos, &["a", "b"]).await;
    repos
        .cluster_membership
        .mark_evicted(&[ab])
        .await
        .expect("close");
    let evicted = repos
        .cluster
        .find_by_ids(&[ab])
        .await
        .expect("load")
        .pop()
        .expect("row");

    let confirmed = repos
        .cluster_membership
        .confirm_many(&[ab], fixed_time())
        .await
        .expect("confirm_many");
    let trimmed = repos
        .cluster_membership
        .trim_and_confirm_many(
            &[ClusterVersionUpdate {
                cluster_id: ab,
                chunks: &[chunk(&["a"], 500, 400)],
            }],
            fixed_time(),
        )
        .await
        .expect("trim_and_confirm_many");

    assert_eq!((confirmed, trimmed), (0, 0));
    let after = repos
        .cluster
        .find_by_ids(&[ab])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(
        (
            after.status,
            after.version,
            after.confirmed_at,
            after.closed_at
        ),
        (ClusterStatus::Evicted, 1, None, evicted.closed_at),
        "evicted means it left the mempool; a later confirm must not relabel it"
    );
}

#[derive(QueryableByName)]
struct RowVersion {
    #[diesel(sql_type = diesel::sql_types::Text)]
    xmin: String,
}

async fn tx_row_versions(pool: &DbPool, ids: &[&str]) -> Vec<String> {
    let mut conn = pool.get().await.expect("conn");
    diesel::sql_query(
        "SELECT xmin::text AS xmin FROM transactions WHERE txid = ANY($1) ORDER BY txid",
    )
    .bind::<diesel::sql_types::Array<diesel::sql_types::Text>, _>(txids(ids))
    .load::<RowVersion>(&mut conn)
    .await
    .expect("load xmin")
    .into_iter()
    .map(|row| row.xmin)
    .collect()
}

#[tokio::test]
async fn unchanged_replace_rewrites_no_transactions_row() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    let cluster = repos
        .cluster_membership
        .insert_with_members(&ClusterFixture::new(&["a", "b"]).build_new_cluster())
        .await
        .expect("insert");
    let before = tx_row_versions(&pool, &["a", "b"]).await;

    repos
        .cluster_membership
        .replace_chunks(ClusterVersionUpdate {
            cluster_id: cluster.id,
            chunks: &cluster.chunks,
        })
        .await
        .expect("replace");

    assert_eq!(tx_row_versions(&pool, &["a", "b"]).await, before);
}

#[tokio::test]
async fn partial_confirm_closes_a_version_of_the_mined_members() {
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
    svc.confirm_mined(&txids(&["a", "b"]), &fees, &weights, fixed_time())
        .await;

    let confirmed = repos
        .cluster
        .find_by_ids(&[original.id])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(confirmed.status, ClusterStatus::Confirmed);
    assert_eq!(confirmed.version, 2);
    assert_eq!(
        confirmed.chunks,
        vec![chunk(&["a", "b"], 1000, 2 * TX_WEIGHT as u64)]
    );
    assert_eq!(confirmed.total_fee, 1000);

    // the pending pair gets its own cluster
    let pending = repos
        .cluster
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    assert_ne!(pending.id, original.id);
    assert_eq!(pending.version, 1);
    assert_eq!(sorted(&members(&pending)), txids(&["c", "d"]));
    assert_eq!(pending.total_fee, 800);
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn partial_confirm_keeps_wholly_mined_chunks_as_the_node_valued_them() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b", "c"]).await;

    // the block mines the chunks around b, which stays in the mempool alone
    let svc = cluster_service(
        pool.clone(),
        vec![
            node_cluster(vec![
                chunk(&["a"], 900, 600),
                chunk(&["b"], 500, 600),
                chunk(&["c"], 300, 600),
            ]),
            ClusterFixture::new(&["b"]).build(),
        ],
    );
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let original = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    // base fees and raw weights, unlike the node's chunk values
    let fees = HashMap::from([("a".to_string(), 700i64), ("c".to_string(), 250i64)]);
    let weights = HashMap::from([("a".to_string(), 560i64), ("c".to_string(), 570i64)]);
    svc.confirm_mined(&txids(&["a", "c"]), &fees, &weights, fixed_time())
        .await;

    let confirmed = repos
        .cluster
        .find_by_ids(&[original.id])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(confirmed.status, ClusterStatus::Confirmed);
    assert_eq!(confirmed.version, 2);
    assert_eq!(
        confirmed.chunks,
        vec![chunk(&["a"], 900, 600), chunk(&["c"], 300, 600)]
    );
    assert_eq!((confirmed.total_fee, confirmed.total_weight), (1200, 1200));
    let positions: Vec<i16> = chunk_rows(&pool)
        .await
        .iter()
        .filter(|row| row.cluster_id == original.id && row.version == 2)
        .map(|row| row.position)
        .collect();
    assert_eq!(positions, vec![0, 1]);

    let pending = repos
        .cluster
        .find_by_txid("b")
        .await
        .expect("query")
        .expect("exists");
    assert_ne!(pending.id, original.id);
    assert_eq!(sorted(&members(&pending)), txids(&["b"]));
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn partial_confirm_values_the_mined_part_of_a_split_chunk_by_the_block() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b", "c", "d"]).await;

    // the block mines a and the b half of {b, c}; c and d stay in the mempool
    let svc = cluster_service(
        pool.clone(),
        vec![
            node_cluster(vec![
                chunk(&["a"], 900, 600),
                chunk(&["b", "c"], 1000, 1200),
                chunk(&["d"], 100, 600),
            ]),
            ClusterFixture::new(&["c", "d"]).build(),
        ],
    );
    svc.sync_clusters_for(&["a".into()], &[]).await;
    let original = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    let fees = HashMap::from([("a".to_string(), 700i64), ("b".to_string(), 450i64)]);
    let weights = HashMap::from([("a".to_string(), 560i64), ("b".to_string(), 580i64)]);
    svc.confirm_mined(&txids(&["a", "b"]), &fees, &weights, fixed_time())
        .await;

    let confirmed = repos
        .cluster
        .find_by_ids(&[original.id])
        .await
        .expect("load")
        .pop()
        .expect("row");
    assert_eq!(confirmed.status, ClusterStatus::Confirmed);
    assert_eq!(confirmed.version, 2);
    assert_eq!(
        confirmed.chunks,
        vec![chunk(&["a"], 900, 600), chunk(&["b"], 450, 580)]
    );
    assert_eq!((confirmed.total_fee, confirmed.total_weight), (1350, 1180));

    let pending = repos
        .cluster
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    assert_ne!(pending.id, original.id);
    assert_eq!(sorted(&members(&pending)), txids(&["c", "d"]));
    assert_live_matches_state(&repos, &pool).await;
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
    assert_eq!(members(&shrunk), txids(&["a", "b"]));
    assert_eq!(shrunk.total_fee, 2 * TX_FEE);
    assert_eq!(shrunk.total_weight, 2 * TX_WEIGHT);
    assert_eq!(
        versions(&chunk_rows(&pool).await, stored.id),
        vec![vec![txids(&["a", "b", "c"])], vec![txids(&["a", "b"])]]
    );

    // evicted tx detached, cluster still active
    assert_eq!(cluster_link(&pool, "c").await, None);
    assert_eq!(repos.cluster.find_active().await.expect("active").len(), 1);
    assert_live_matches_state(&repos, &pool).await;
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

    // a is alone now, but alone is a cluster: a new version instead of a close
    assert_eq!(
        versions(&chunk_rows(&pool).await, stored.id),
        vec![vec![txids(&["a", "b"])], vec![txids(&["a"])]]
    );
    assert_eq!(cluster_link(&pool, "a").await, Some(stored.id));
    assert_eq!(cluster_link(&pool, "b").await, None);
    let active = repos.cluster.find_active().await.expect("active");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, stored.id);
    assert_live_matches_state(&repos, &pool).await;
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

    // nothing left to belong to the cluster, so it closes on its last version
    let closed = repos
        .cluster
        .find_by_ids(&[stored.id])
        .await
        .expect("query")
        .pop()
        .expect("row kept");
    assert_eq!(closed.status, ClusterStatus::Evicted);
    assert!(closed.closed_at.is_some());
    assert_eq!(closed.version, 1);
    assert_eq!(members(&closed), txids(&["a", "b"]));
    assert_eq!(closed.total_fee, 1000);
    assert_eq!(closed.total_weight, 2 * TX_WEIGHT);
    assert_eq!(cluster_link(&pool, "a").await, None);
    assert_eq!(cluster_link(&pool, "b").await, None);
    assert!(
        repos
            .cluster
            .find_active()
            .await
            .expect("active")
            .is_empty()
    );
    assert_live_matches_state(&repos, &pool).await;
}

#[tokio::test]
async fn evicting_the_child_of_two_parents_splits_the_cluster() {
    let pool = isolated_pool().await;
    let repos = Repos::new(pool.clone());
    seed_sized_txs(&repos.transaction, &["a", "b", "c"]).await;

    let retriever = MockClusterRetriever::with_clusters(vec![
        ClusterFixture::new(&["a", "c", "b"])
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

    // b spent both a and c, the only link between them
    retriever.set_clusters(vec![
        ClusterFixture::new(&["a"]).build(),
        ClusterFixture::new(&["c"]).build(),
    ]);
    svc.sync_clusters_for(&[], &["b".into()]).await;

    let active = repos.cluster.find_active().await.expect("active");
    let mut groups: Vec<Vec<String>> = active.iter().map(members).collect();
    groups.sort();
    assert_eq!(groups, vec![txids(&["a"]), txids(&["c"])]);

    // the original row keeps one side, the other side gets a row of its own
    assert!(active.iter().any(|c| c.id == stored.id));
    assert_eq!(
        cluster_link(&pool, "b").await,
        None,
        "the evicted tx must not stay linked"
    );
    for txid in ["a", "c"] {
        assert!(
            cluster_link(&pool, txid).await.is_some(),
            "{txid} must belong to a cluster"
        );
    }
    assert_live_matches_state(&repos, &pool).await;
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
    svc.confirm_mined(&txids(&["a", "b"]), &fees, &weights, fixed_time())
        .await;
    let rows_before = chunk_rows(&pool).await.len();
    let before = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    // confirmed members keep their back-link; eviction must not touch the cluster
    svc.sync_clusters_for(&[], &["a".into()]).await;

    assert_eq!(chunk_rows(&pool).await.len(), rows_before);
    let after = repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("row kept with members");
    assert_eq!(
        (
            after.status,
            after.version,
            after.closed_at,
            members(&after)
        ),
        (
            before.status,
            before.version,
            before.closed_at,
            members(&before)
        )
    );
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
    assert_eq!(members(&shrunk), txids(&["a", "b"]));

    // websocket subscribers see the shrink as an upserted frame
    let frame = tokio::time::timeout(Duration::from_secs(1), frames.next())
        .await
        .expect("frame within timeout")
        .expect("stream open");
    assert_eq!(frame.upserted.len(), 1);
    assert_eq!(frame.upserted[0].id, stored.id);
    assert_eq!(sorted(&frame.upserted[0].txids), txids(&["a", "b"]));
}
