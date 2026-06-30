#![cfg(feature = "db_integration_tests")]

use api::db::models::NewTransaction;
use api::db::{ClusterRepository, DbPool, TransactionRepository};
use api::services::cluster::ClusterService;
use api::services::cluster_delta::{ClusterDeltaService, ClusterSnapshot};
use api::services::pubsub::PubSubService;
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
        TransactionRepository::new(pool),
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

#[tokio::test]
async fn stores_multi_tx_cluster_and_links_member_txs() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b"]).await;

    let retriever = MockClusterRetriever::with_clusters(vec![cluster(&["a", "b"], 1500)]);
    let svc = ClusterService::new(
        cluster_repo.clone(),
        tx_repo.clone(),
        retriever.clone(),
        ClusterDeltaService::new(
            ClusterSnapshot::default(),
            PubSubService::new(PubSub::new()),
        ),
    );

    svc.sync_clusters_for(&["a".into(), "b".into()]).await;

    // one cluster fetch only, b is covered by a's cluster (dedup)
    assert_eq!(retriever.clusters_fetched(), vec!["a".to_string()]);

    let stored = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");
    let mut txids = stored.txids.clone();
    txids.sort();
    assert_eq!(txids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(stored.total_fee, 1500);
    assert_eq!(stored.total_vsize, 200);

    // both member txs link to the cluster
    let ids = tx_repo
        .get_cluster_ids_by_txids(&["a".into(), "b".into()])
        .await
        .expect("ids");
    assert_eq!(ids, vec![stored.id]);
}

#[tokio::test]
async fn skips_singleton_clusters() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["c"]).await;

    // default mock reports every tx as a singleton
    let svc = service(pool, vec![]);
    svc.sync_clusters_for(&["c".into()]).await;

    assert_eq!(cluster_repo.count().await.expect("count"), 0);
    assert!(
        tx_repo
            .get_cluster_ids_by_txids(&["c".into()])
            .await
            .expect("ids")
            .is_empty()
    );
}

#[tokio::test]
async fn updates_existing_cluster_when_group_grows() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b", "c"]).await;

    service(pool.clone(), vec![cluster(&["a", "b"], 1000)])
        .sync_clusters_for(&["a".into(), "b".into()])
        .await;
    let first = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    // c joins the cluster
    service(pool, vec![cluster(&["a", "b", "c"], 1500)])
        .sync_clusters_for(&["c".into()])
        .await;

    assert_eq!(cluster_repo.count().await.expect("count"), 1);
    let updated = cluster_repo
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(updated.id, first.id); // same row, updated in place
    assert_eq!(updated.total_fee, 1500);
    assert_eq!(updated.total_vsize, 300);
    assert_eq!(updated.txids.len(), 3);
}

#[tokio::test]
async fn clears_orphan_cluster_id_when_member_leaves_cluster() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b", "c"]).await;

    // initial mempool cluster {a,b,c}; all three member txs get linked
    service(pool.clone(), vec![cluster(&["a", "b", "c"], 1500)])
        .sync_clusters_for(&["a".into()])
        .await;
    let initial = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(initial.txids.len(), 3);
    assert_eq!(
        tx_repo
            .get_cluster_ids_by_txids(&["c".into()])
            .await
            .expect("ids"),
        vec![initial.id]
    );

    // c drops out of the mempool cluster, and the retriever now reports {a,b}
    service(pool, vec![cluster(&["a", "b"], 1000)])
        .sync_clusters_for(&["a".into()])
        .await;

    // cluster row correctly reports {a,b}, same id
    let shrunk = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    let mut txids = shrunk.txids.clone();
    txids.sort();
    assert_eq!(txids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(shrunk.id, initial.id);

    // c no longer belongs to any cluster, so its back-reference must be cleared
    let c_links = tx_repo
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
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b", "c", "d"]).await;

    // two separate clusters first
    service(
        pool.clone(),
        vec![cluster(&["a", "b"], 1000), cluster(&["c", "d"], 800)],
    )
    .sync_clusters_for(&["a".into(), "c".into()])
    .await;
    assert_eq!(cluster_repo.count().await.expect("count"), 2);

    // {a,b} was observed before {c,d}, so it carries the older first_seen_at
    let ab = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    let cd = cluster_repo
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    let oldest_first_seen = ab.first_seen_at;
    assert!(oldest_first_seen < cd.first_seen_at);

    // a linking tx merges them
    service(pool, vec![cluster(&["a", "b", "c", "d"], 1800)])
        .sync_clusters_for(&["a".into()])
        .await;

    assert_eq!(cluster_repo.count().await.expect("count"), 1);
    let merged = cluster_repo
        .find_by_txid("d")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(merged.total_fee, 1800);
    assert_eq!(merged.total_vsize, 400);
    assert_eq!(merged.txids.len(), 4);
    assert_eq!(merged.first_seen_at, oldest_first_seen);

    // all four txs point at the single surviving cluster
    let ids = tx_repo
        .get_cluster_ids_by_txids(&["a".into(), "b".into(), "c".into(), "d".into()])
        .await
        .expect("ids");
    assert_eq!(ids, vec![merged.id]);
}
