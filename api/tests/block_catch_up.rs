#![cfg(feature = "db_integration_tests")]

//! A block event triggers a catch-up from our highest block to the node's tip,
//! rather than applying just the block it names. See issue #15.

use api::db::DbPool;
use api::db::schema::{blocks, transactions};
use api::services::block::BlockService;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::events::BlockConnectedEvent;
use shared::models::GetBlockModel;
use testkit::deps::deps;
use testkit::fixtures::BlockFixture;
use testkit::mocks::{MockBlockRetriever, MockClusterRetriever};
use testkit::postgres::isolated_pool;

fn chain(heights: std::ops::RangeInclusive<i64>) -> Vec<GetBlockModel> {
    heights
        .map(|h| {
            BlockFixture::new(&format!("blk{h}"), h)
                .with_txs(&[(&format!("tx{h}"), 100)])
                .build()
        })
        .collect()
}

fn event(hash: &str) -> BlockConnectedEvent {
    BlockConnectedEvent { hash: hash.into() }
}

fn service(
    pool: &DbPool,
    retriever: &MockBlockRetriever,
) -> BlockService<MockBlockRetriever, MockClusterRetriever> {
    deps(pool.clone())
        .with_cluster_retriever(MockClusterRetriever::default())
        .with_block_retriever(retriever.clone())
        .block_service()
}

async fn stored_heights(pool: &DbPool) -> Vec<i64> {
    let mut conn = pool.get().await.expect("conn");
    blocks::table
        .order(blocks::height.asc())
        .select(blocks::height)
        .load(&mut conn)
        .await
        .expect("load heights")
}

async fn confirmed_in(pool: &DbPool, txid: &str) -> Option<String> {
    let mut conn = pool.get().await.expect("conn");
    transactions::table
        .find(txid)
        .select(transactions::confirmed_at_block)
        .first(&mut conn)
        .await
        .expect("load tx")
}

#[tokio::test]
async fn an_event_backfills_the_heights_whose_notifications_were_lost() {
    let pool = isolated_pool().await;
    let retriever = MockBlockRetriever::with_blocks(chain(100..=102));
    let service = service(&pool, &retriever);
    service
        .apply_block(event("blk100"))
        .await
        .expect("seed 100");

    // 101 was never announced
    service
        .persist_blocks_and_txs(futures::stream::iter([event("blk102")]))
        .await;

    assert_eq!(stored_heights(&pool).await, vec![100, 101, 102]);
    assert_eq!(confirmed_in(&pool, "tx101").await, Some("blk101".into()));
    assert_eq!(
        retriever.blocks_fetched(),
        vec!["blk100", "blk101", "blk102"],
        "applied in height order"
    );
}

#[tokio::test]
async fn a_failing_block_is_retried_without_skipping_past_it() {
    let pool = isolated_pool().await;
    let retriever = MockBlockRetriever::with_blocks(chain(100..=102)).failing("blk101", 2);
    let service = service(&pool, &retriever);
    service
        .apply_block(event("blk100"))
        .await
        .expect("seed 100");

    service
        .persist_blocks_and_txs(futures::stream::iter([event("blk102")]))
        .await;

    assert_eq!(stored_heights(&pool).await, vec![100, 101, 102]);
    assert_eq!(
        retriever.blocks_fetched(),
        vec!["blk100", "blk101", "blk101", "blk101", "blk102"],
        "102 must wait until 101 lands"
    );
}

#[tokio::test]
async fn an_event_for_a_block_already_stored_fetches_nothing() {
    let pool = isolated_pool().await;
    let retriever = MockBlockRetriever::with_blocks(chain(100..=100));
    let service = service(&pool, &retriever);
    service
        .apply_block(event("blk100"))
        .await
        .expect("seed 100");

    service
        .persist_blocks_and_txs(futures::stream::iter([event("blk100")]))
        .await;

    assert_eq!(retriever.blocks_fetched(), vec!["blk100"]);
}

#[tokio::test]
async fn bootstrap_sync_waits_for_a_failing_block_to_land() {
    let pool = isolated_pool().await;
    let retriever = MockBlockRetriever::with_blocks(chain(100..=102)).failing("blk101", 2);
    let service = service(&pool, &retriever);
    service
        .apply_block(event("blk100"))
        .await
        .expect("seed 100");

    service.sync_missing_blocks().await;

    assert_eq!(stored_heights(&pool).await, vec![100, 101, 102]);
    assert_eq!(
        retriever.blocks_fetched(),
        vec!["blk100", "blk101", "blk101", "blk101", "blk102"]
    );
}
