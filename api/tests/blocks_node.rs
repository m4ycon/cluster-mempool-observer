#![cfg(all(feature = "db_integration_tests", feature = "node_integration_tests"))]

use api::db::schema::{blocks, transactions};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::events::BlockConnectedEvent;
use testkit::deps::deps_for_node;
use testkit::node::{maturate_coinbase, send_to_address, setup_node};
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

#[tokio::test]
async fn applies_a_real_mined_block_end_to_end() {
    let pool = isolated_pool().await;
    let node = setup_node();
    let service = deps_for_node(pool.clone(), &node).block_service();

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
        .await
        .expect("apply block");

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
