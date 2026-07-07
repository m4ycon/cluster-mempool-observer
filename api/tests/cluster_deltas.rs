#![cfg(feature = "db_integration_tests")]

use api::db::models::{ClusterDelta, NewTransaction};
use api::db::schema::cluster_deltas;
use api::db::{ClusterMembershipRepository, ClusterRepository, DbPool, TransactionRepository};
use api::services::cluster::ClusterService;
use api::services::cluster_delta::{ClusterDeltaService, ClusterSnapshot};
use api::services::pubsub::PubSubService;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::models::GetMempoolClusterModel;
use shared::pubsub::PubSub;
use std::collections::HashMap;
use testkit::mocks::MockClusterRetriever;
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

fn cluster(txids: &[&str], total_fee_sats: u64) -> GetMempoolClusterModel {
    GetMempoolClusterModel {
        cluster_weight: 400 * txids.len() as u64,
        tx_count: txids.len() as u32,
        txids: txids.iter().map(|s| s.to_string()).collect(),
        total_fee_sats,
    }
}

fn service(
    pool: DbPool,
    clusters: Vec<GetMempoolClusterModel>,
) -> ClusterService<MockClusterRetriever> {
    ClusterService::new(
        ClusterRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        ClusterMembershipRepository::new(pool),
        MockClusterRetriever::with_clusters(clusters),
        ClusterDeltaService::new(
            ClusterSnapshot::default(),
            PubSubService::new(PubSub::new()),
        ),
    )
}

async fn seed_txs(repo: &TransactionRepository, txids: &[&str]) {
    for txid in txids {
        repo.insert(&NewTransaction::hollow(txid))
            .await
            .expect("seed tx");
    }
}

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
fn assert_closed_in_log(rows: &[ClusterDelta], cluster_id: i64) {
    let rows: Vec<&ClusterDelta> = rows.iter().filter(|r| r.cluster_id == cluster_id).collect();
    assert!(!rows.is_empty(), "no log rows for cluster {cluster_id}");
    let fee_sum: i64 = rows.iter().map(|r| r.fee_delta).sum();
    let vsize_sum: i64 = rows.iter().map(|r| r.vsize_delta).sum();
    assert_eq!(fee_sum, 0, "fee deltas of cluster {cluster_id} must sum to 0");
    assert_eq!(vsize_sum, 0, "vsize deltas of cluster {cluster_id} must sum to 0");

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

fn confirmed_at() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
}

#[tokio::test]
async fn new_cluster_logs_added_members_and_positive_deltas() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b"]).await;

    service(pool.clone(), vec![cluster(&["a", "b"], 1500)])
        .sync_clusters_for(&["a".into()])
        .await;

    let stored = cluster_repo
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
    assert_eq!(rows[0].vsize_delta, 200);
}

#[tokio::test]
async fn growth_logs_only_the_joined_member() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b", "c"]).await;

    service(pool.clone(), vec![cluster(&["a", "b"], 1000)])
        .sync_clusters_for(&["a".into()])
        .await;
    service(pool.clone(), vec![cluster(&["a", "b", "c"], 1500)])
        .sync_clusters_for(&["c".into()])
        .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].added_txids, vec!["c"]);
    assert!(rows[1].removed_txids.is_empty());
    assert_eq!(rows[1].fee_delta, 500);
    assert_eq!(rows[1].vsize_delta, 100);
}

#[tokio::test]
async fn departure_logs_only_the_removed_member() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b", "c"]).await;

    service(pool.clone(), vec![cluster(&["a", "b", "c"], 1500)])
        .sync_clusters_for(&["a".into()])
        .await;
    service(pool.clone(), vec![cluster(&["a", "b"], 1000)])
        .sync_clusters_for(&["a".into()])
        .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert_eq!(rows[1].removed_txids, vec!["c"]);
    assert_eq!(rows[1].fee_delta, -500);
    assert_eq!(rows[1].vsize_delta, -100);
}

#[tokio::test]
async fn unchanged_resync_logs_nothing() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b"]).await;

    service(pool.clone(), vec![cluster(&["a", "b"], 1500)])
        .sync_clusters_for(&["a".into()])
        .await;
    service(pool.clone(), vec![cluster(&["a", "b"], 1500)])
        .sync_clusters_for(&["b".into()])
        .await;

    assert_eq!(delta_rows(&pool).await.len(), 1);
}

#[tokio::test]
async fn merge_closes_loser_before_keeper_absorbs_members() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b", "c", "d"]).await;

    service(
        pool.clone(),
        vec![cluster(&["a", "b"], 1000), cluster(&["c", "d"], 800)],
    )
    .sync_clusters_for(&["a".into(), "c".into()])
    .await;
    let keeper = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    let loser = cluster_repo
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");

    service(pool.clone(), vec![cluster(&["a", "b", "c", "d"], 1800)])
        .sync_clusters_for(&["a".into()])
        .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 4);

    // loser's closing row comes before the keeper's absorption row
    assert_eq!(rows[2].cluster_id, loser.id);
    assert!(rows[2].added_txids.is_empty());
    assert_eq!(sorted(&rows[2].removed_txids), vec!["c", "d"]);
    assert_eq!(rows[2].fee_delta, -800);
    assert_eq!(rows[2].vsize_delta, -200);

    assert_eq!(rows[3].cluster_id, keeper.id);
    assert_eq!(sorted(&rows[3].added_txids), vec!["c", "d"]);
    assert!(rows[3].removed_txids.is_empty());
    assert_eq!(rows[3].fee_delta, 800);
    assert_eq!(rows[3].vsize_delta, 200);

    assert_closed_in_log(&rows, loser.id);
    let active = cluster_repo.find_active().await.expect("active");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, keeper.id);
}

#[tokio::test]
async fn full_confirm_logs_closing_row_once() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b"]).await;

    let svc = service(pool.clone(), vec![cluster(&["a", "b"], 1500)]);
    svc.sync_clusters_for(&["a".into()]).await;
    let stored = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    let fees = HashMap::from([("a".to_string(), 800i64), ("b".to_string(), 700i64)]);
    let sizes = HashMap::from([("a".to_string(), 100i64), ("b".to_string(), 100i64)]);
    let mined: Vec<String> = vec!["a".into(), "b".into()];
    svc.confirm_mined(&mined, &fees, &sizes, confirmed_at())
        .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert_eq!(sorted(&rows[1].removed_txids), vec!["a", "b"]);
    assert_eq!(rows[1].fee_delta, -1500);
    assert_eq!(rows[1].vsize_delta, -200);
    assert_closed_in_log(&rows, stored.id);

    // re-confirming must not log a second closing row
    svc.confirm_mined(&mined, &fees, &sizes, confirmed_at())
        .await;
    assert_eq!(delta_rows(&pool).await.len(), 2);

    // confirmed member txs keep their cluster back-link
    assert_eq!(
        tx_repo
            .get_cluster_ids_by_txids(&mined)
            .await
            .expect("ids"),
        vec![stored.id]
    );
}

#[tokio::test]
async fn partial_confirm_logs_departures_then_closing_row() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b", "c", "d"]).await;

    // initial cluster {a,b,c,d}; after {a,b} confirm, the retriever reports
    // the still-pending {c,d} as their own cluster
    let svc = service(
        pool.clone(),
        vec![cluster(&["a", "b", "c", "d"], 1800), cluster(&["c", "d"], 800)],
    );
    svc.sync_clusters_for(&["a".into()]).await;
    let original = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    let fees = HashMap::from([("a".to_string(), 600i64), ("b".to_string(), 400i64)]);
    let sizes = HashMap::from([("a".to_string(), 150i64), ("b".to_string(), 150i64)]);
    svc.confirm_mined(&["a".into(), "b".into()], &fees, &sizes, confirmed_at())
        .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 4);

    // still-pending members leave first, with the confirmed-subset totals
    assert_eq!(rows[1].cluster_id, original.id);
    assert_eq!(sorted(&rows[1].removed_txids), vec!["c", "d"]);
    assert_eq!(rows[1].fee_delta, 1000 - 1800);
    assert_eq!(rows[1].vsize_delta, 300 - 400);

    // then the confirm closes the original cluster
    assert_eq!(rows[2].cluster_id, original.id);
    assert_eq!(sorted(&rows[2].removed_txids), vec!["a", "b"]);
    assert_eq!(rows[2].fee_delta, -1000);
    assert_eq!(rows[2].vsize_delta, -300);
    assert_closed_in_log(&rows, original.id);

    // the pending pair gets its own cluster with its own opening row
    let pending = cluster_repo
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    assert_ne!(pending.id, original.id);
    assert_eq!(rows[3].cluster_id, pending.id);
    assert_eq!(sorted(&rows[3].added_txids), vec!["c", "d"]);
    assert_eq!(rows[3].fee_delta, 800);
    assert_eq!(rows[3].vsize_delta, 200);
}

#[tokio::test]
async fn fee_only_change_logs_empty_arrays_with_fee_delta() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b"]).await;

    service(pool.clone(), vec![cluster(&["a", "b"], 1000)])
        .sync_clusters_for(&["a".into()])
        .await;
    service(pool.clone(), vec![cluster(&["a", "b"], 1500)])
        .sync_clusters_for(&["a".into()])
        .await;

    let rows = delta_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[1].added_txids.is_empty());
    assert!(rows[1].removed_txids.is_empty());
    assert_eq!(rows[1].fee_delta, 500);
    assert_eq!(rows[1].vsize_delta, 0);
}
