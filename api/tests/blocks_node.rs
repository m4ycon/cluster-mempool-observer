#![cfg(all(feature = "db_integration_tests", feature = "node_integration_tests"))]

use api::db::schema::{blocks, transactions};
use api::db::{
    BlockRepository, ClusterMembershipRepository, ClusterRepository, MempoolDeltaRepository,
    TransactionRepository,
};
use api::services::block::BlockService;
use api::services::cluster::ClusterService;
use api::services::cluster_delta::ClusterDeltaService;
use api::services::pubsub::PubSubService;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use observer::retrievers::{BlockRpcRetriever, ClusterRpcRetriever, MempoolRetriever};
use shared::events::BlockConnectedEvent;
use shared::pubsub::PubSub;
use shared::snapshot::ClusterSnapshot;
use shared::snapshot::MempoolSnapshot;
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

#[tokio::test]
async fn applies_a_real_mined_block_end_to_end() {
    let pool = isolated_pool().await;
    let (node, rpc) = setup_node_and_rpc_client();

    let service = BlockService::new(
        BlockRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        MempoolDeltaRepository::new(pool.clone()),
        ClusterService::new(
            ClusterRepository::new(pool.clone()),
            TransactionRepository::new(pool.clone()),
            ClusterMembershipRepository::new(pool.clone()),
            ClusterRpcRetriever::new(rpc.clone()),
            ClusterDeltaService::new(
                ClusterSnapshot::default(),
                PubSubService::new(PubSub::new()),
            ),
        ),
        BlockRpcRetriever::new(rpc.clone()),
        MempoolRetriever::new(rpc.clone(), MempoolSnapshot::default()),
        PubSubService::new(PubSub::new()),
    );

    // fund the wallet, broadcast a tx, then mine it into a block
    let address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &address);
    let spent = send_to_address(&node, &address);
    let generated = node
        .client
        .generate_to_address(1, &address)
        .expect("mine 1 block");
    let mined_hash = generated.0.into_iter().next().expect("one block hash");

    service
        .apply_block(BlockConnectedEvent {
            hash: mined_hash.clone(),
        })
        .await;

    let mut conn = pool.get().await.expect("conn");

    // the block record was persisted (height 102: 101 maturity + 1)
    let (height, tx_count): (i64, i64) = blocks::table
        .find(&mined_hash)
        .select((blocks::height, blocks::tx_count))
        .first(&mut conn)
        .await
        .expect("load block row");
    assert_eq!(height, 102);
    assert!(tx_count >= 2, "coinbase + the spend tx");

    // the broadcast tx is now stored and confirmed
    let confirmed_at: Option<OffsetDateTime> = transactions::table
        .find(spent.to_string())
        .select(transactions::confirmed_at)
        .first(&mut conn)
        .await
        .expect("load spent tx");
    assert!(confirmed_at.is_some(), "mined tx should be confirmed");
}
