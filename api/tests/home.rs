#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, NewBlock, NewMempoolDelta};
use shared::events::ClusterRef;
use std::collections::HashSet;
use testkit::deps::deps;
use testkit::postgres::isolated_pool;
use time::OffsetDateTime;

fn cluster_ref(id: i64) -> ClusterRef {
    ClusterRef {
        id,
        txids: vec![format!("tx{id}")],
        total_vsize: 1,
        total_fee: 1,
    }
}

#[tokio::test]
async fn current_stats_aggregates_live_counters() {
    let deps = deps(isolated_pool().await);

    // two recent add deltas within the 60s window => tx_per_min == 2
    deps.repos
        .mempool_delta
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

    deps.mempool_snapshot.store(
        ["t1", "t2", "t3"]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>(),
    );
    deps.cluster_snapshot.seed((1..=2).map(cluster_ref));

    let stats = deps.home_service().current_stats().await;

    assert_eq!(stats.mempool_size, 3);
    assert_eq!(stats.cluster_count, 2);
    assert_eq!(stats.tx_per_min, 2);
}

#[tokio::test]
async fn get_current_chain_tip_reflects_latest_block() {
    let deps = deps(isolated_pool().await);
    let home = deps.home_service();

    // no blocks yet
    assert!(home.get_current_chain_tip().await.is_none());

    let when = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    deps.repos
        .block
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
