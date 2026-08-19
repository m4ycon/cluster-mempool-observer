#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, NewTransaction};
use api::db::schema::{mempool_deltas, transactions};
use api::infra::deps::Deps;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use observer::retrievers::{BlockRetriever, ClusterRetriever, TransactionRetriever};
use shared::events::MempoolDeltaEvent;
use testkit::deps::{deps, isolated_deps};
use testkit::fixtures::{MempoolEntryFixture, RawTxFixture, fixed_time};
use testkit::mocks::{MockClusterRetriever, MockTransactionRetriever};
use testkit::postgres::isolated_pool;

use std::collections::HashMap;

/// (txid, fee, vsize, hollow, input_txids)
type TxRow = (String, Option<i64>, i64, bool, Option<Vec<String>>);

/// Drains every txid currently queued for backfill, in FIFO order.
fn drain_enqueued_txids<TR: TransactionRetriever, CR: ClusterRetriever, BR: BlockRetriever>(
    deps: &Deps<TR, CR, BR>,
) -> Vec<String> {
    let mut rx = deps
        .tx_backfill_queue
        .take_receiver()
        .expect("receiver taken exactly once");
    let mut txids = Vec::new();
    while let Ok(req) = rx.try_recv() {
        txids.push(req.txid);
    }
    txids
}

#[tokio::test]
async fn streamed_deltas_are_persisted() {
    let deps = isolated_deps()
        .await
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default());
    let mempool_service = deps.mempool_service();

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
    assert_eq!(
        deps.repos
            .mempool_delta
            .count()
            .await
            .expect("count deltas"),
        3
    );
    // folding those rows leaves only a in the mempool
    assert_eq!(
        deps.repos
            .mempool_delta
            .reconstruct_snapshot()
            .await
            .expect("reconstruct"),
        std::collections::HashSet::from(["a".to_string()])
    );
}

#[tokio::test]
async fn removal_of_confirmed_tx_is_recorded_as_remove_confirmed() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone())
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default());
    let mempool_service = deps.mempool_service();

    // the tx enters the mempool
    mempool_service
        .apply_delta(MempoolDeltaEvent {
            added: vec!["mined".into()],
            removed: vec![],
        })
        .await;

    // the block path confirms it before the removal delta is processed
    let mut confirmed_tx = NewTransaction::from(&RawTxFixture::new("mined").build());
    confirmed_tx.confirmed_at = Some(fixed_time());
    deps.repos
        .transaction
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
    let deps = isolated_deps()
        .await
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default());
    let mempool_service = deps.mempool_service();

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
    assert_eq!(
        deps.repos
            .mempool_delta
            .count()
            .await
            .expect("count deltas"),
        2
    );
    assert_eq!(
        deps.repos
            .mempool_delta
            .reconstruct_snapshot()
            .await
            .expect("reconstruct"),
        std::collections::HashSet::new()
    );
}

#[tokio::test]
async fn persist_deltas_inserts_new_transactions_hollow_and_enqueues_them() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone()).with_cluster_retriever(MockClusterRetriever::default());
    let mempool_service = deps.mempool_service();

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["tx1".into(), "tx2".into()],
        removed: vec![],
    }]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    let rows: Vec<(String, bool, Option<Vec<String>>)> = {
        let mut conn = pool.get().await.expect("conn");
        transactions::table
            .order(transactions::txid.asc())
            .select((
                transactions::txid,
                transactions::hollow,
                transactions::input_txids,
            ))
            .load(&mut conn)
            .await
            .expect("load transactions")
    };
    assert_eq!(
        rows,
        vec![
            ("tx1".to_string(), true, None),
            ("tx2".to_string(), true, None),
        ]
    );

    let mut enqueued = drain_enqueued_txids(&deps);
    enqueued.sort();
    assert_eq!(enqueued, vec!["tx1".to_string(), "tx2".to_string()]);
}

#[tokio::test]
async fn persist_deltas_skips_already_known_transactions() {
    let deps = isolated_deps()
        .await
        .with_cluster_retriever(MockClusterRetriever::default());
    let mempool_service = deps.mempool_service();

    deps.repos
        .transaction
        .insert(&NewTransaction::from(&RawTxFixture::new("known").build()))
        .await
        .expect("seed known tx");

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["known".into(), "fresh".into()],
        removed: vec![],
    }]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    let mut stored = deps
        .repos
        .transaction
        .existing_txids(&["known".to_string(), "fresh".to_string()])
        .await
        .expect("query existing");
    stored.sort();
    assert_eq!(stored, vec!["fresh".to_string(), "known".to_string()]);

    assert_eq!(drain_enqueued_txids(&deps), vec!["fresh".to_string()]);
}

#[tokio::test]
async fn persist_deltas_inserts_txids_without_entries_hollow() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone()).with_cluster_retriever(MockClusterRetriever::default());
    let mempool_service = deps.mempool_service();

    let source = futures::stream::iter(vec![MempoolDeltaEvent {
        added: vec!["ok".into(), "broken".into()],
        removed: vec![],
    }]);

    mempool_service.persist_deltas_and_new_txs(source).await;

    // this stage never calls getrawtransaction any more, so there is no
    // "fetch failed" case left to distinguish -- every txid without an entry
    // lands hollow, parents not known yet
    let rows: Vec<(String, bool, Option<Vec<String>>)> = {
        let mut conn = pool.get().await.expect("conn");
        transactions::table
            .order(transactions::txid.asc())
            .select((
                transactions::txid,
                transactions::hollow,
                transactions::input_txids,
            ))
            .load(&mut conn)
            .await
            .expect("load transactions")
    };
    assert_eq!(
        rows,
        vec![
            ("broken".to_string(), true, None),
            ("ok".to_string(), true, None),
        ]
    );
}

#[tokio::test]
async fn bootstrap_delta_builds_txs_from_entries_without_fetching() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone()).with_cluster_retriever(MockClusterRetriever::default());
    let mempool_service = deps.mempool_service();

    let entries = HashMap::from([
        (
            "a".to_string(),
            MempoolEntryFixture::new("a")
                .with_fee_in_sats(700)
                .with_vsize(140)
                .build(),
        ),
        (
            "b".to_string(),
            MempoolEntryFixture::new("b")
                .with_fee_in_sats(900)
                .with_vsize(220)
                .build(),
        ),
    ]);

    mempool_service
        .apply_bootstrap_delta(
            MempoolDeltaEvent {
                added: vec!["a".into(), "b".into()],
                removed: vec![],
            },
            entries,
        )
        .await;

    let rows: Vec<TxRow> = {
        let mut conn = pool.get().await.expect("conn");
        transactions::table
            .order(transactions::txid.asc())
            .select((
                transactions::txid,
                transactions::fee,
                transactions::vsize,
                transactions::hollow,
                transactions::input_txids,
            ))
            .load(&mut conn)
            .await
            .expect("load transactions")
    };
    assert_eq!(
        rows,
        vec![
            ("a".to_string(), Some(700), 140, false, None),
            ("b".to_string(), Some(900), 220, false, None),
        ],
        "fee and vsize come straight from the entries, nothing hollow, parents not known yet"
    );

    // an entry gives fee/vsize but never parents, so it still queues for
    // getrawtransaction enrichment just like a hollow row would
    let mut enqueued = drain_enqueued_txids(&deps);
    enqueued.sort();
    assert_eq!(enqueued, vec!["a".to_string(), "b".to_string()]);
}

#[tokio::test]
async fn bootstrap_delta_inserts_hollow_for_txids_without_an_entry() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone()).with_cluster_retriever(MockClusterRetriever::default());
    let mempool_service = deps.mempool_service();

    let entries = HashMap::from([(
        "known".to_string(),
        MempoolEntryFixture::new("known").build(),
    )]);

    mempool_service
        .apply_bootstrap_delta(
            MempoolDeltaEvent {
                added: vec!["known".into(), "orphan".into()],
                removed: vec![],
            },
            entries,
        )
        .await;

    let rows: Vec<TxRow> = {
        let mut conn = pool.get().await.expect("conn");
        transactions::table
            .order(transactions::txid.asc())
            .select((
                transactions::txid,
                transactions::fee,
                transactions::vsize,
                transactions::hollow,
                transactions::input_txids,
            ))
            .load(&mut conn)
            .await
            .expect("load transactions")
    };
    assert_eq!(
        rows,
        vec![
            ("known".to_string(), Some(500), 100, false, None),
            ("orphan".to_string(), None, 0, true, None),
        ]
    );
}
