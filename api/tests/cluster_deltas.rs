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
use testkit::mocks::MockClusterRetriever;
use testkit::postgres::isolated_pool;

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
