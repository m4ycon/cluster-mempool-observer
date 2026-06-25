#![cfg(all(feature = "db_integration_tests", feature = "node_integration_tests"))]

use api::db::models::NewBlock;
use api::db::schema::blocks;
use api::db::{BlockRepository, ClusterRepository, TransactionRepository};
use api::services::block::BlockService;
use api::services::cluster::ClusterService;
use api::services::pubsub::PubSubService;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use observer::retrievers::{BlockRpcRetriever, ClusterRpcRetriever};
use shared::pubsub::PubSub;
use testkit::node::{maturate_coinbase, setup_node_and_rpc_client};
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

fn build_service(
    pool: api::db::DbPool,
    rpc: observer::clients::rpc_client::RpcClient,
) -> BlockService<BlockRpcRetriever, ClusterRpcRetriever> {
    BlockService::new(
        BlockRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        ClusterService::new(
            ClusterRepository::new(pool.clone()),
            TransactionRepository::new(pool.clone()),
            ClusterRpcRetriever::new(rpc.clone()),
        ),
        BlockRpcRetriever::new(rpc.clone()),
        PubSubService::new(PubSub::new()),
    )
}

#[tokio::test]
async fn backfills_blocks_missed_while_down() {
    let pool = isolated_pool().await;
    let (node, rpc) = setup_node_and_rpc_client();
    let service = build_service(pool.clone(), rpc);

    // tip 101 after maturity
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);

    // pretend the DB already processed up to height 101
    BlockRepository::new(pool.clone())
        .insert(&NewBlock {
            hash: "seed-block-101".to_string(),
            height: 101,
            mined_at: OffsetDateTime::now_utc(),
            tx_count: 0,
            total_size: 0,
            total_fee: 0,
            difficulty: 0.0,
        })
        .await
        .expect("seed block 101");

    // chain advances to 103 while we were "down"
    node.client
        .generate_to_address(2, &address)
        .expect("mine 2 blocks");

    service.sync_missing_blocks().await;

    let mut conn = pool.get().await.expect("conn");

    let max_height: Option<i64> = blocks::table
        .select(diesel::dsl::max(blocks::height))
        .first(&mut conn)
        .await
        .expect("max height");
    assert_eq!(max_height, Some(103), "tip should be persisted");

    for height in [102_i64, 103_i64] {
        blocks::table
            .filter(blocks::height.eq(height))
            .select(blocks::hash)
            .first::<String>(&mut conn)
            .await
            .unwrap_or_else(|e| panic!("height {height} should be backfilled: {e}"));
    }
}

#[tokio::test]
async fn cold_start_syncs_tip_only() {
    let pool = isolated_pool().await;
    let (node, rpc) = setup_node_and_rpc_client();
    let service = build_service(pool.clone(), rpc);

    // tip 104, DB empty
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    node.client
        .generate_to_address(3, &address)
        .expect("mine 3 blocks");

    service.sync_missing_blocks().await;

    let mut conn = pool.get().await.expect("conn");

    let count: i64 = blocks::table
        .count()
        .get_result(&mut conn)
        .await
        .expect("count blocks");
    assert_eq!(count, 1, "cold start should persist only the tip block");

    let height: i64 = blocks::table
        .select(blocks::height)
        .first(&mut conn)
        .await
        .expect("the one block height");
    assert_eq!(height, 104, "the single persisted block is the tip");
}
