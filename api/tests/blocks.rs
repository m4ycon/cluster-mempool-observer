#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, NewTransaction};
use api::db::schema::{blocks, mempool_deltas, transactions};
use api::db::{
    BlockRepository, ClusterMembershipRepository, ClusterRepository, DbPool,
    MempoolDeltaRepository, TransactionRepository,
};
use api::services::block::BlockService;
use api::services::cluster::ClusterService;
use api::services::cluster_delta::{ClusterDeltaService, ClusterSnapshot};
use api::services::pubsub::PubSubService;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use observer::clients::rpc_client::RpcClient;
use observer::infra::config::RpcConfig;
use observer::retrievers::MempoolRetriever;
use observer::snapshot::MempoolSnapshot;
use shared::events::BlockConnectedEvent;
use shared::models::{BlockTxSummary, GetBlockModel, GetMempoolClusterModel};
use shared::pubsub::PubSub;
use testkit::mocks::{MockBlockRetriever, MockClusterRetriever};
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

fn mined_at() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
}

fn build_block(
    hash: &str,
    height: i64,
    mined_at: OffsetDateTime,
    txs: &[(&str, i64)],
) -> GetBlockModel {
    GetBlockModel {
        hash: hash.to_string(),
        height,
        mined_at,
        size: 1_000,
        difficulty: 2.0,
        txs: txs
            .iter()
            .map(|(txid, fee)| BlockTxSummary {
                txid: txid.to_string(),
                vsize: 100,
                fee_sats: *fee,
            })
            .collect(),
    }
}

fn mempool_cluster(txids: &[&str], total_fee_sats: u64) -> GetMempoolClusterModel {
    GetMempoolClusterModel {
        cluster_weight: 400 * txids.len() as u64,
        tx_count: txids.len() as u32,
        txids: txids.iter().map(|s| s.to_string()).collect(),
        total_fee_sats,
    }
}

/// A `MempoolRetriever` whose in-memory snapshot is seeded with `txids`.
/// The RPC client is never called by the block path.
fn mempool_retriever(txids: &[&str]) -> MempoolRetriever {
    let snapshot = MempoolSnapshot::default();
    snapshot.store(txids.iter().map(|s| s.to_string()).collect());
    let rpc = RpcClient::new(&RpcConfig {
        host: "127.0.0.1:1".into(),
        user: String::new(),
        pass: String::new(),
    })
    .expect("rpc client builds without connecting");
    MempoolRetriever::new(rpc, snapshot)
}

fn block_service(
    pool: DbPool,
    blocks: Vec<GetBlockModel>,
    clusters: Vec<GetMempoolClusterModel>,
    mempool_txids: &[&str],
) -> BlockService<MockBlockRetriever, MockClusterRetriever> {
    let cluster_service = ClusterService::new(
        ClusterRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        ClusterMembershipRepository::new(pool.clone()),
        MockClusterRetriever::with_clusters(clusters),
        ClusterDeltaService::new(
            ClusterSnapshot::default(),
            PubSubService::new(PubSub::new()),
        ),
    );
    BlockService::new(
        BlockRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        MempoolDeltaRepository::new(pool.clone()),
        cluster_service,
        MockBlockRetriever::with_blocks(blocks),
        mempool_retriever(mempool_txids),
        PubSubService::new(PubSub::new()),
    )
}

async fn seed_txs(repo: &TransactionRepository, txids: &[&str]) {
    for txid in txids {
        repo.insert(&NewTransaction::hollow(txid))
            .await
            .expect("seed tx");
    }
}

/// (confirmed_at, first_seen_at, fee, cluster_id)
async fn tx_row(
    pool: &DbPool,
    txid: &str,
) -> (
    Option<OffsetDateTime>,
    OffsetDateTime,
    Option<i64>,
    Option<i64>,
    Option<String>,
) {
    let mut conn = pool.get().await.expect("conn");
    transactions::table
        .filter(transactions::txid.eq(txid))
        .select((
            transactions::confirmed_at,
            transactions::first_seen_at,
            transactions::fee,
            transactions::cluster_id,
            transactions::confirmed_at_block,
        ))
        .first(&mut conn)
        .await
        .expect("load tx row")
}

#[tokio::test]
async fn persists_block_and_confirms_new_and_existing_txs() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());

    // an already-tracked tx seen earlier in the mempool, with no fee yet
    let earlier = OffsetDateTime::UNIX_EPOCH;
    tx_repo
        .insert(&NewTransaction {
            txid: "seen".into(),
            fee: None,
            vsize: 99,
            first_seen_at: earlier,
            confirmed_at: None,
            cluster_id: None,
            confirmed_at_block: None,
        })
        .await
        .expect("seed seen tx");

    let when = mined_at();
    let block = build_block("blk1", 100, when, &[("seen", 500), ("fresh", 700)]);
    block_service(pool.clone(), vec![block], vec![], &[])
        .apply_block(BlockConnectedEvent {
            hash: "blk1".into(),
        })
        .await;

    // block record persisted with aggregates (scope the conn: the size-1 test
    // pool would deadlock if it were still checked out during tx_row below)
    let (height, tx_count, total_fee, total_bytes, difficulty): (i64, i64, i64, i64, f64) = {
        let mut conn = pool.get().await.expect("conn");
        blocks::table
            .find("blk1")
            .select((
                blocks::height,
                blocks::tx_count,
                blocks::total_fee,
                blocks::total_bytes,
                blocks::difficulty,
            ))
            .first(&mut conn)
            .await
            .expect("load block")
    };
    assert_eq!(height, 100);
    assert_eq!(tx_count, 2);
    assert_eq!(total_fee, 1_200);
    assert_eq!(total_bytes, 1_000);
    assert_eq!(difficulty, 2.0);

    // existing tx: confirmed + fee filled, but first_seen_at preserved
    let (confirmed, first_seen, fee, _cluster, confirmed_block) = tx_row(&pool, "seen").await;
    assert_eq!(confirmed, Some(when));
    assert_eq!(first_seen, earlier);
    assert_eq!(fee, Some(500));
    assert_eq!(confirmed_block, Some("blk1".to_string()));

    // brand-new tx: inserted confirmed at block time
    let (confirmed, first_seen, fee, _cluster, confirmed_block) = tx_row(&pool, "fresh").await;
    assert_eq!(confirmed, Some(when));
    assert_eq!(first_seen, when);
    assert_eq!(fee, Some(700));
    assert_eq!(confirmed_block, Some("blk1".to_string()));
}

#[tokio::test]
async fn fully_mined_cluster_is_confirmed() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b"]).await;

    // build the pending cluster {a,b}
    ClusterService::new(
        cluster_repo.clone(),
        tx_repo.clone(),
        ClusterMembershipRepository::new(pool.clone()),
        MockClusterRetriever::with_clusters(vec![mempool_cluster(&["a", "b"], 1000)]),
        ClusterDeltaService::new(
            ClusterSnapshot::default(),
            PubSubService::new(PubSub::new()),
        ),
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;

    let when = mined_at();
    let block = build_block("blk", 1, when, &[("a", 100), ("b", 200)]);
    block_service(pool.clone(), vec![block], vec![], &[])
        .apply_block(BlockConnectedEvent { hash: "blk".into() })
        .await;

    let cluster = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("cluster exists");
    let mut txids = cluster.txids.clone();
    txids.sort();
    assert_eq!(txids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(cluster.confirmed_at, Some(when));
}

#[tokio::test]
async fn partially_mined_cluster_splits() {
    let pool = isolated_pool().await;
    let tx_repo = TransactionRepository::new(pool.clone());
    let cluster_repo = ClusterRepository::new(pool.clone());
    seed_txs(&tx_repo, &["a", "b", "c", "d"]).await;

    // build the pending cluster {a,b,c,d}
    ClusterService::new(
        cluster_repo.clone(),
        tx_repo.clone(),
        ClusterMembershipRepository::new(pool.clone()),
        MockClusterRetriever::with_clusters(vec![mempool_cluster(&["a", "b", "c", "d"], 1100)]),
        ClusterDeltaService::new(
            ClusterSnapshot::default(),
            PubSubService::new(PubSub::new()),
        ),
    )
    .sync_clusters_for(&["a".into()], &[])
    .await;
    let original = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    // block mines a,b; the live mempool now clusters the remaining {c,d}
    let when = mined_at();
    let block = build_block("blk", 1, when, &[("a", 100), ("b", 200)]);
    block_service(
        pool.clone(),
        vec![block],
        vec![mempool_cluster(&["c", "d"], 800)],
        &[],
    )
    .apply_block(BlockConnectedEvent { hash: "blk".into() })
    .await;

    // original row keeps only the mined members and is confirmed
    let confirmed = cluster_repo
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");
    assert_eq!(confirmed.id, original.id);
    let mut txids = confirmed.txids.clone();
    txids.sort();
    assert_eq!(txids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(confirmed.confirmed_at, Some(when));
    assert_eq!(confirmed.total_fee, 300);

    // remainder re-clustered into a new, still-pending cluster
    let pending = cluster_repo
        .find_by_txid("c")
        .await
        .expect("query")
        .expect("exists");
    assert_ne!(pending.id, original.id);
    let mut txids = pending.txids.clone();
    txids.sort();
    assert_eq!(txids, vec!["c".to_string(), "d".to_string()]);
    assert_eq!(pending.confirmed_at, None);
    assert_eq!(pending.total_fee, 800);

    // tx links follow the split
    let (_, _, _, a_cluster_id, _) = tx_row(&pool, "a").await;
    let (_, _, _, c_cluster_id, _) = tx_row(&pool, "c").await;
    assert_eq!(a_cluster_id, Some(confirmed.id));
    assert_eq!(c_cluster_id, Some(pending.id));
}

#[tokio::test]
async fn mined_mempool_txs_get_remove_confirmed_delta() {
    let pool = isolated_pool().await;

    let when = mined_at();
    let block = build_block("blk", 1, when, &[("seen", 500), ("fresh", 700)]);
    // only "seen" was in our mempool snapshot; "fresh" was never seen
    block_service(pool.clone(), vec![block], vec![], &["seen"])
        .apply_block(BlockConnectedEvent { hash: "blk".into() })
        .await;

    // only the in-mempool tx yields a remove_confirmed delta row
    let rows: Vec<(String, DeltaReason)> = {
        let mut conn = pool.get().await.expect("conn");
        mempool_deltas::table
            .select((mempool_deltas::txid, mempool_deltas::reason))
            .load(&mut conn)
            .await
            .expect("load deltas")
    };
    assert_eq!(
        rows,
        vec![("seen".to_string(), DeltaReason::RemoveConfirmed)]
    );
}
