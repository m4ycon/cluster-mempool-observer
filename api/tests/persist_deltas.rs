#![cfg(feature = "db_integration_tests")]

mod common;

use api::db::models::{DeltaReason, NewTransaction};
use api::db::schema::mempool_deltas;
use api::db::{
    ClusterMembershipRepository, ClusterRepository, MempoolDeltaRepository, TransactionRepository,
};
use api::services::cluster::ClusterService;
use api::services::cluster_delta::ClusterDeltaService;
use api::services::mempool::MempoolService;
use api::services::pubsub::PubSubService;
use common::dummy_tx;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::events::MempoolDeltaEvent;
use shared::pubsub::PubSub;
use shared::snapshot::ClusterSnapshot;
use testkit::mocks::{MockClusterRetriever, MockTransactionRetriever};
use testkit::postgres::isolated_pool;

fn build_cluster_service(pool: api::db::DbPool) -> ClusterService<MockClusterRetriever> {
    ClusterService::new(
        ClusterRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        ClusterMembershipRepository::new(pool),
        MockClusterRetriever::default(),
        ClusterDeltaService::new(
            ClusterSnapshot::default(),
            PubSubService::new(PubSub::new()),
        ),
    )
}

#[tokio::test]
async fn streamed_deltas_are_persisted() {
    let pool = isolated_pool().await;
    let mempool_delta_repo = MempoolDeltaRepository::new(pool.clone());
    let mempool_service = MempoolService::new(
        mempool_delta_repo.clone(),
        TransactionRepository::new(pool.clone()),
        MockTransactionRetriever::default(),
        build_cluster_service(pool),
        PubSubService::new(PubSub::new()),
    );

    let source = futures::stream::iter(vec![
        // add a & b -> two add_mempool rows, both stored as unconfirmed txs
        MempoolDeltaEvent {
            added: vec!["a".into(), "b".into()],
            removed: vec![],
        },
        // b leaves the mempool unconfirmed -> one remove_evicted row
        MempoolDeltaEvent {
            added: vec![],
            removed: vec!["b".into()],
        },
    ]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    // 2 add_mempool + 1 remove_evicted
    assert_eq!(mempool_delta_repo.count().await.expect("count deltas"), 3);
    // folding those rows leaves only a in the mempool
    assert_eq!(
        mempool_delta_repo
            .reconstruct_snapshot()
            .await
            .expect("reconstruct"),
        std::collections::HashSet::from(["a".to_string()])
    );
}

#[tokio::test]
async fn removal_of_confirmed_tx_is_recorded_as_remove_confirmed() {
    let pool = isolated_pool().await;
    let mempool_delta_repo = MempoolDeltaRepository::new(pool.clone());
    let transaction_repo = TransactionRepository::new(pool.clone());
    let mempool_service = MempoolService::new(
        mempool_delta_repo.clone(),
        transaction_repo.clone(),
        MockTransactionRetriever::default(),
        build_cluster_service(pool.clone()),
        PubSubService::new(PubSub::new()),
    );

    // the tx enters the mempool
    mempool_service
        .apply_delta(MempoolDeltaEvent {
            added: vec!["mined".into()],
            removed: vec![],
        })
        .await;

    // the block path confirms it before the removal delta is processed
    let mut confirmed_tx = NewTransaction::from(&dummy_tx("mined"));
    confirmed_tx.confirmed_at =
        Some(time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap());
    transaction_repo
        .insert_or_confirm_many(&[confirmed_tx])
        .await
        .expect("confirm tx");

    // the watcher reports it gone from the mempool
    mempool_service
        .apply_delta(MempoolDeltaEvent {
            added: vec![],
            removed: vec!["mined".into()],
        })
        .await;

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
            ("mined".to_string(), DeltaReason::AddMempool),
            ("mined".to_string(), DeltaReason::RemoveConfirmed),
        ]
    );
}

#[tokio::test]
async fn unmatched_and_duplicate_removals_write_no_rows() {
    let pool = isolated_pool().await;
    let mempool_delta_repo = MempoolDeltaRepository::new(pool.clone());
    let mempool_service = MempoolService::new(
        mempool_delta_repo.clone(),
        TransactionRepository::new(pool.clone()),
        MockTransactionRetriever::default(),
        build_cluster_service(pool),
        PubSubService::new(PubSub::new()),
    );

    let source = futures::stream::iter(vec![
        // "ghost" was never added -> its removal is skipped
        MempoolDeltaEvent {
            added: vec!["a".into()],
            removed: vec!["ghost".into()],
        },
        // a leaves -> one remove_evicted
        MempoolDeltaEvent {
            added: vec![],
            removed: vec!["a".into()],
        },
        // duplicate removal of a -> already paired, skipped
        MempoolDeltaEvent {
            added: vec![],
            removed: vec!["a".into()],
        },
    ]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    // 1 add + 1 remove_evicted, nothing else
    assert_eq!(mempool_delta_repo.count().await.expect("count deltas"), 2);
    assert_eq!(
        mempool_delta_repo
            .reconstruct_snapshot()
            .await
            .expect("reconstruct"),
        std::collections::HashSet::new()
    );
}

#[tokio::test]
async fn persist_deltas_fetches_and_stores_new_transactions() {
    let pool = isolated_pool().await;
    let transaction_repo = TransactionRepository::new(pool.clone());
    let mock_transaction_retriever = MockTransactionRetriever::default();
    let mempool_service = MempoolService::new(
        MempoolDeltaRepository::new(pool.clone()),
        transaction_repo.clone(),
        mock_transaction_retriever.clone(),
        build_cluster_service(pool),
        PubSubService::new(PubSub::new()),
    );

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["tx1".into(), "tx2".into()],
        removed: vec![],
    }]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    assert_eq!(
        mock_transaction_retriever.txs_fetched(),
        vec!["tx1".to_string(), "tx2".to_string()]
    );

    let mut stored = transaction_repo
        .existing_txids(&["tx1".to_string(), "tx2".to_string()])
        .await
        .expect("query existing");
    stored.sort();
    assert_eq!(stored, vec!["tx1".to_string(), "tx2".to_string()]);
}

#[tokio::test]
async fn persist_deltas_skips_already_known_transactions() {
    let pool = isolated_pool().await;
    let transaction_repo = TransactionRepository::new(pool.clone());
    let mock_transaction_retriever = MockTransactionRetriever::default();
    let mempool_service = MempoolService::new(
        MempoolDeltaRepository::new(pool.clone()),
        transaction_repo.clone(),
        mock_transaction_retriever.clone(),
        build_cluster_service(pool),
        PubSubService::new(PubSub::new()),
    );

    transaction_repo
        .insert(&NewTransaction::from(&dummy_tx("known")))
        .await
        .expect("seed known tx");

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["known".into(), "fresh".into()],
        removed: vec![],
    }]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    assert_eq!(
        mock_transaction_retriever.txs_fetched(),
        vec!["fresh".to_string()]
    );

    let mut stored = transaction_repo
        .existing_txids(&["known".to_string(), "fresh".to_string()])
        .await
        .expect("query existing");
    stored.sort();
    assert_eq!(stored, vec!["fresh".to_string(), "known".to_string()]);
}

#[tokio::test]
async fn persist_deltas_stores_hollow_tx_on_retrieval_error() {
    let pool = isolated_pool().await;
    let transaction_repo = TransactionRepository::new(pool.clone());
    let mempool_service = MempoolService::new(
        MempoolDeltaRepository::new(pool.clone()),
        transaction_repo.clone(),
        MockTransactionRetriever::failing_for(["broken".to_string()]),
        build_cluster_service(pool),
        PubSubService::new(PubSub::new()),
    );

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["ok".into(), "broken".into()],
        removed: vec![],
    }]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    // both txids are recorded, the failing one as a hollow placeholder
    let mut stored = transaction_repo
        .existing_txids(&["ok".to_string(), "broken".to_string()])
        .await
        .expect("query existing");
    stored.sort();
    assert_eq!(stored, vec!["broken".to_string(), "ok".to_string()]);
}
