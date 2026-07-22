#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, NewBlock, NewMempoolDelta};
use api::db::{
    BlockRepository, ClusterMembershipRepository, ClusterRepository, DbPool,
    MempoolDeltaRepository, TransactionRepository,
};
use api::services::cluster::ClusterService;
use api::services::cluster_delta::ClusterDeltaService;
use api::services::home::HomeService;
use api::services::pubsub::PubSubService;
use observer::clients::rpc_client::RpcClient;
use observer::infra::config::RpcConfig;
use observer::retrievers::{ClusterRpcRetriever, MempoolRetriever};
use shared::events::ClusterRef;
use shared::pubsub::PubSub;
use shared::snapshot::{ClusterSnapshot, MempoolSnapshot};
use std::collections::HashSet;
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

fn inert_rpc() -> RpcClient {
    // deadpool/rpc are constructed without a handshake, so nothing connects
    RpcClient::new(&RpcConfig {
        host: "127.0.0.1:1".into(),
        user: String::new(),
        pass: String::new(),
    })
    .expect("rpc client builds without connecting")
}

fn cluster_ref(id: i64) -> ClusterRef {
    ClusterRef {
        id,
        txids: vec![format!("tx{id}")],
        total_vsize: 1,
        total_fee: 1,
    }
}

/// Builds a `HomeService` whose live mempool set and active-cluster snapshot are
/// seeded in memory; DB-backed reads still go through `pool`.
fn home_service(pool: DbPool, mempool_txids: &[&str], clusters: usize) -> HomeService {
    let rpc = inert_rpc();
    let pubsub = PubSubService::new(PubSub::new());

    let mempool_snapshot = MempoolSnapshot::default();
    mempool_snapshot.store(
        mempool_txids
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>(),
    );
    let mempool_retriever = MempoolRetriever::new(rpc.clone(), mempool_snapshot);

    let cluster_snapshot = ClusterSnapshot::default();
    cluster_snapshot.seed((1..=clusters as i64).map(cluster_ref));
    let cluster_service = ClusterService::new(
        ClusterRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        ClusterMembershipRepository::new(pool.clone()),
        ClusterRpcRetriever::new(rpc),
        ClusterDeltaService::new(cluster_snapshot, pubsub.clone()),
    );

    HomeService::new(
        BlockRepository::new(pool.clone()),
        MempoolDeltaRepository::new(pool),
        mempool_retriever,
        cluster_service,
        pubsub,
    )
}

#[tokio::test]
async fn current_stats_aggregates_live_counters() {
    let pool = isolated_pool().await;

    // two recent add deltas within the 60s window => tx_per_min == 2
    MempoolDeltaRepository::new(pool.clone())
        .insert_many(&[
            NewMempoolDelta {
                txid: "x".into(),
                reason: DeltaReason::AddMempool,
            },
            NewMempoolDelta {
                txid: "y".into(),
                reason: DeltaReason::AddMempool,
            },
        ])
        .await
        .expect("seed adds");

    let home = home_service(pool.clone(), &["t1", "t2", "t3"], 2);
    let stats = home.current_stats().await;

    assert_eq!(stats.mempool_size, 3);
    assert_eq!(stats.cluster_count, 2);
    assert_eq!(stats.tx_per_min, 2);
}

#[tokio::test]
async fn get_current_chain_tip_reflects_latest_block() {
    let pool = isolated_pool().await;
    let home = home_service(pool.clone(), &[], 0);

    // no blocks yet
    assert!(home.get_current_chain_tip().await.is_none());

    let when = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    BlockRepository::new(pool.clone())
        .insert(&NewBlock {
            hash: "b".into(),
            height: 800_000,
            mined_at: when,
            tx_count: 1,
            total_bytes: 1,
            total_fee: 1,
            difficulty: 1.0,
        })
        .await
        .expect("insert block");

    let tip = home.get_current_chain_tip().await.expect("tip");
    assert_eq!(tip.height, 800_000);
    assert_eq!(tip.mined_at, when);
}
