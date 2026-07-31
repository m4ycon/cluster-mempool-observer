#![cfg(feature = "db_integration_tests")]

use api::db::models::DeltaReason;
use api::db::schema::{blocks, mempool_deltas, transactions};
use api::db::{BlockRepository, DbPool, MempoolDeltaRepository, TransactionRepository};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::events::BlockConnectedEvent;
use testkit::deps::{block_service, deps};
use testkit::fixtures::{
    BlockFixture, ClusterFixture, MempoolDeltaFixture, NewBlockFixture, TxFixture, fixed_time,
    seed_txs,
};
use testkit::mocks::MockClusterRetriever;
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

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
        .insert(&TxFixture::new("seen").with_first_seen_at(earlier).build())
        .await
        .expect("seed seen tx");

    let when = fixed_time();
    let block = BlockFixture::new("blk1", 100)
        .with_mined_at(when)
        .with_txs(&[("seen", 500), ("fresh", 700)])
        .build();
    block_service(pool.clone(), vec![block], vec![])
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
    let deps =
        deps(pool.clone()).with_cluster_retriever(MockClusterRetriever::with_clusters(vec![
            ClusterFixture::new(&["a", "b"])
                .with_total_fee_sats(1000)
                .build(),
        ]));
    seed_txs(&deps.repos.transaction, &["a", "b"]).await;

    // build the pending cluster {a,b}
    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;

    let when = fixed_time();
    let block = BlockFixture::new("blk", 1)
        .with_mined_at(when)
        .with_txs(&[("a", 100), ("b", 200)])
        .build();
    block_service(pool.clone(), vec![block], vec![])
        .apply_block(BlockConnectedEvent { hash: "blk".into() })
        .await;

    let cluster = deps
        .repos
        .cluster
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
    let deps =
        deps(pool.clone()).with_cluster_retriever(MockClusterRetriever::with_clusters(vec![
            ClusterFixture::new(&["a", "b", "c", "d"])
                .with_total_fee_sats(1100)
                .build(),
        ]));
    seed_txs(&deps.repos.transaction, &["a", "b", "c", "d"]).await;

    // build the pending cluster {a,b,c,d}
    deps.cluster_service()
        .sync_clusters_for(&["a".into()], &[])
        .await;
    let original = deps
        .repos
        .cluster
        .find_by_txid("a")
        .await
        .expect("query")
        .expect("exists");

    // block mines a,b; the live mempool now clusters the remaining {c,d}
    let when = fixed_time();
    let block = BlockFixture::new("blk", 1)
        .with_mined_at(when)
        .with_txs(&[("a", 100), ("b", 200)])
        .build();
    block_service(
        pool.clone(),
        vec![block],
        vec![
            ClusterFixture::new(&["c", "d"])
                .with_total_fee_sats(800)
                .build(),
        ],
    )
    .apply_block(BlockConnectedEvent { hash: "blk".into() })
    .await;

    // original row keeps only the mined members and is confirmed
    let confirmed = deps
        .repos
        .cluster
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
    let pending = deps
        .repos
        .cluster
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
    let delta_repo = MempoolDeltaRepository::new(pool.clone());

    // only "seen" entered the mempool (has an unpaired add); "fresh" was never seen
    delta_repo
        .insert_many(&[MempoolDeltaFixture::added("seen").build()])
        .await
        .expect("seed add delta");

    let when = fixed_time();
    let block = BlockFixture::new("blk", 1)
        .with_mined_at(when)
        .with_txs(&[("seen", 500), ("fresh", 700)])
        .build();
    block_service(pool.clone(), vec![block], vec![])
        .apply_block(BlockConnectedEvent { hash: "blk".into() })
        .await;

    // only the in-mempool tx yields a remove_confirmed delta row
    let rows: Vec<(String, DeltaReason)> = {
        let mut conn = pool.get().await.expect("conn");
        mempool_deltas::table
            .order(mempool_deltas::id.asc())
            .select((mempool_deltas::txid, mempool_deltas::reason))
            .load(&mut conn)
            .await
            .expect("load deltas")
    };
    assert_eq!(
        rows,
        vec![
            ("seen".to_string(), DeltaReason::AddMempool),
            ("seen".to_string(), DeltaReason::RemoveConfirmed),
        ]
    );
}

#[tokio::test]
async fn mined_tx_already_removed_gets_no_second_remove() {
    let pool = isolated_pool().await;
    let delta_repo = MempoolDeltaRepository::new(pool.clone());

    // "seen" entered and already left the mempool (e.g. persister won the race)
    delta_repo
        .insert_many(&[
            MempoolDeltaFixture::added("seen").build(),
            MempoolDeltaFixture::new("seen", DeltaReason::RemoveEvicted).build(),
        ])
        .await
        .expect("seed paired deltas");

    let when = fixed_time();
    let block = BlockFixture::new("blk", 1)
        .with_mined_at(when)
        .with_txs(&[("seen", 500)])
        .build();
    block_service(pool.clone(), vec![block], vec![])
        .apply_block(BlockConnectedEvent { hash: "blk".into() })
        .await;

    let count: i64 = {
        let mut conn = pool.get().await.expect("conn");
        mempool_deltas::table
            .count()
            .get_result(&mut conn)
            .await
            .expect("count deltas")
    };
    assert_eq!(count, 2, "no extra remove row for an already-paired add");
}

#[tokio::test]
async fn latest_returns_highest_block_height_and_mined_at() {
    let pool = isolated_pool().await;
    let repo = BlockRepository::new(pool.clone());

    // empty table
    assert!(repo.latest().await.expect("latest").is_none());

    // insert out of order; latest must follow height, not insertion order
    let later = OffsetDateTime::from_unix_timestamp(1_700_000_600).unwrap();
    repo.insert(&NewBlockFixture::new("b2", 101).with_mined_at(later).build())
        .await
        .expect("insert b2");
    repo.insert(&NewBlockFixture::new("b1", 100).build())
        .await
        .expect("insert b1");

    let (height, at) = repo.latest().await.expect("latest").expect("some block");
    assert_eq!(height, 101);
    assert_eq!(at, later);
}
