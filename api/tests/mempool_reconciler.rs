#![cfg(feature = "db_integration_tests")]

//! `MempoolReconciler` is the single writer of `mempool_deltas` (step 3 of
//! `docs/plano-mempool-ledger.md`). These tests drive it directly through the
//! `MempoolLedger` it consumes, never through a generator -- that wiring is
//! step 4.

use api::db::models::{ClusterStatus, DeltaReason, NewTransaction};
use api::db::schema::{mempool_deltas, transactions};
use api::db::{MempoolLedgerRepository, TRANSACTION_INSERT_CHUNK_SIZE};
use api::infra::deps::Deps;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use futures::StreamExt;
use observer::retrievers::{BlockRetriever, ClusterRetriever, TransactionRetriever};
use shared::events::MempoolDeltaEvent;
use shared::models::DeltaDirection;
use shared::snapshot::JournalEntry;
use shared::subjects::Subject;
use testkit::deps::deps;
use testkit::fixtures::{ClusterFixture, RawTxFixture, fixed_time};
use testkit::mocks::{MockClusterRetriever, MockTransactionRetriever};
use testkit::postgres::isolated_pool;

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

/// All `mempool_deltas` rows for `txid`, oldest first -- `id` order is the
/// journal order, which is what the ordering-trap tests below assert on.
async fn delta_reasons_in_order(pool: &api::db::DbPool, txid: &str) -> Vec<DeltaReason> {
    let mut conn = pool.get().await.expect("checkout connection");
    mempool_deltas::table
        .filter(mempool_deltas::txid.eq(txid))
        .order(mempool_deltas::id.asc())
        .select(mempool_deltas::reason)
        .load(&mut conn)
        .await
        .expect("load deltas")
}

fn base_deps(
    pool: api::db::DbPool,
) -> Deps<MockTransactionRetriever, MockClusterRetriever, observer::retrievers::BlockRpcRetriever> {
    deps(pool)
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::default())
}

#[tokio::test]
async fn one_add_writes_a_row_a_hollow_tx_and_enqueues_backfill() {
    let pool = isolated_pool().await;
    let deps = base_deps(pool.clone());
    let reconciler = deps.mempool_reconciler();

    deps.mempool_ledger.assert_present(&["a".to_string()]);
    reconciler.tick().await;

    assert_eq!(
        delta_reasons_in_order(&pool, "a").await,
        vec![DeltaReason::AddMempool]
    );

    let mut conn = pool.get().await.expect("checkout connection");
    let rows: Vec<(bool, Option<i64>)> = transactions::table
        .filter(transactions::txid.eq("a"))
        .select((transactions::hollow, transactions::fee))
        .load(&mut conn)
        .await
        .expect("load transactions");
    assert_eq!(rows, vec![(true, None)], "a hollow row, fee unknown");

    assert_eq!(drain_enqueued_txids(&deps), vec!["a".to_string()]);
    assert_eq!(deps.mempool_ledger.pending_len(), 0);
}

#[tokio::test]
async fn add_of_a_tx_that_already_exists_writes_no_new_tx_row_and_enqueues_nothing() {
    let pool = isolated_pool().await;
    let deps = base_deps(pool.clone());

    deps.repos
        .transaction
        .insert(&NewTransaction::from(&RawTxFixture::new("known").build()))
        .await
        .expect("seed known tx");

    let reconciler = deps.mempool_reconciler();
    deps.mempool_ledger.assert_present(&["known".to_string()]);
    reconciler.tick().await;

    assert_eq!(
        delta_reasons_in_order(&pool, "known").await,
        vec![DeltaReason::AddMempool]
    );

    let mut conn = pool.get().await.expect("checkout connection");
    let row_count: i64 = transactions::table
        .filter(transactions::txid.eq("known"))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count transactions");
    assert_eq!(row_count, 1, "no second row for the already-known tx");

    assert!(
        drain_enqueued_txids(&deps).is_empty(),
        "an already-known tx needs no backfill"
    );
}

#[tokio::test]
async fn remove_reason_follows_confirmed_at_and_only_evicted_reaches_cluster_sync() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone())
        .with_transaction_retriever(MockTransactionRetriever::default())
        .with_cluster_retriever(MockClusterRetriever::with_clusters(vec![
            ClusterFixture::new(&["confirmed_tx"]).build(),
            ClusterFixture::new(&["evicted_tx"]).build(),
        ]));
    let reconciler = deps.mempool_reconciler();

    // both enter and get their own single-tx cluster
    deps.mempool_ledger
        .assert_present(&["confirmed_tx".to_string(), "evicted_tx".to_string()]);
    reconciler.tick().await;

    // the block path confirms one of them before it leaves the mempool
    let mut confirmed = NewTransaction::from(&RawTxFixture::new("confirmed_tx").build());
    confirmed.confirmed_at = Some(fixed_time());
    deps.repos
        .transaction
        .insert_or_confirm_many(&[confirmed])
        .await
        .expect("confirm tx");

    deps.mempool_ledger
        .assert_absent(&["confirmed_tx".to_string(), "evicted_tx".to_string()]);
    reconciler.tick().await;

    assert_eq!(
        delta_reasons_in_order(&pool, "confirmed_tx").await,
        vec![DeltaReason::AddMempool, DeltaReason::RemoveConfirmed]
    );
    assert_eq!(
        delta_reasons_in_order(&pool, "evicted_tx").await,
        vec![DeltaReason::AddMempool, DeltaReason::RemoveEvicted]
    );

    // remove_confirmed never reaches handle_evicted, so that cluster is
    // untouched here -- confirming it is the block path's job, not the
    // reconciler's; remove_evicted does, and closes its cluster
    let confirmed_cluster = deps
        .repos
        .cluster
        .find_by_txid("confirmed_tx")
        .await
        .expect("query")
        .expect("cluster exists");
    assert_eq!(confirmed_cluster.status, ClusterStatus::Active);

    let evicted_cluster = deps
        .repos
        .cluster
        .find_by_txid("evicted_tx")
        .await
        .expect("query")
        .expect("cluster exists");
    assert_eq!(evicted_cluster.status, ClusterStatus::Evicted);
}

#[tokio::test]
async fn add_then_remove_within_one_tick_is_written_in_that_order() {
    let pool = isolated_pool().await;
    // strict, unlike base_deps: this tick's own sync_clusters_for asks the
    // retriever about "t" again after the batch commits, and by then "t" is
    // absent
    let deps = base_deps(pool.clone()).with_cluster_retriever(MockClusterRetriever::strict(vec![]));
    let reconciler = deps.mempool_reconciler();

    deps.mempool_ledger.assert_present(&["t".to_string()]);
    deps.mempool_ledger.assert_absent(&["t".to_string()]);
    reconciler.tick().await;

    assert_eq!(
        delta_reasons_in_order(&pool, "t").await,
        vec![DeltaReason::AddMempool, DeltaReason::RemoveEvicted],
        "must land in journal order, not grouped by direction"
    );
    assert_eq!(deps.mempool_ledger.pending_len(), 0);
}

#[tokio::test]
async fn remove_then_add_within_one_tick_is_written_in_that_order() {
    let pool = isolated_pool().await;
    let deps = base_deps(pool.clone());
    let reconciler = deps.mempool_reconciler();

    deps.mempool_ledger.assert_present(&["t".to_string()]);
    reconciler.tick().await; // commits the initial Add

    // both will be written in the same tick, but the Remove must be first in the journal
    deps.mempool_ledger.assert_absent(&["t".to_string()]);
    deps.mempool_ledger.assert_present(&["t".to_string()]);
    reconciler.tick().await;

    assert_eq!(
        delta_reasons_in_order(&pool, "t").await,
        vec![
            DeltaReason::AddMempool,
            DeltaReason::RemoveEvicted,
            DeltaReason::AddMempool,
        ],
        "must land in journal order: Remove before the re-Add, not the other way round"
    );
    assert_eq!(deps.mempool_ledger.pending_len(), 0);
}

#[tokio::test]
async fn the_journal_is_empty_after_a_successful_tick() {
    let pool = isolated_pool().await;
    let deps = base_deps(pool.clone());
    let reconciler = deps.mempool_reconciler();

    deps.mempool_ledger
        .assert_present(&["a".to_string(), "b".to_string()]);
    assert!(deps.mempool_ledger.pending_len() > 0);

    reconciler.tick().await;

    assert_eq!(deps.mempool_ledger.pending_len(), 0);
}

#[tokio::test]
async fn remove_then_add_within_one_tick_publishes_the_txid_in_neither_list() {
    let pool = isolated_pool().await;
    let deps = base_deps(pool.clone());
    let reconciler = deps.mempool_reconciler();

    deps.mempool_ledger.assert_present(&["x".to_string()]);
    reconciler.tick().await; // commits the initial add; x is resident

    let delta_stream = deps
        .pubsub
        .subscribe::<MempoolDeltaEvent>(Subject::MempoolDelta)
        .await;
    futures::pin_mut!(delta_stream);

    deps.mempool_ledger.assert_absent(&["x".to_string()]);
    deps.mempool_ledger.assert_present(&["x".to_string()]);
    reconciler.tick().await;

    let event = tokio::time::timeout(std::time::Duration::from_millis(200), delta_stream.next())
        .await
        .expect("event published within timeout")
        .expect("stream not closed");

    assert!(
        !event.added.contains(&"x".to_string()) && !event.removed.contains(&"x".to_string()),
        "x never actually changed residency across the batch: {event:?}"
    );
}

/// The chunk loops must sit inside `write_batch`'s transaction, not around it.
/// A refactor that opened one transaction per chunk would still pass every test
/// above -- they all fit in a single chunk -- and would only come apart in
/// production, on the first batch big enough to split, leaving `mempool_deltas`
/// rows whose `transactions` rows were never written.
#[tokio::test]
async fn a_failure_in_the_second_chunk_rolls_back_the_first_and_the_deltas() {
    // Autocommit, unlike the isolated pool the other tests use: the rollback
    // being asserted has to be write_batch's own, not a test transaction's.
    let pool = testkit::postgres::autocommit_pool().await;
    let repo = MempoolLedgerRepository::new(pool.clone());

    let entries: Vec<JournalEntry> = (0..=TRANSACTION_INSERT_CHUNK_SIZE)
        .map(|i| JournalEntry {
            txid: format!("chunk-atomicity-{i}"),
            direction: DeltaDirection::Add,
        })
        .collect();

    // A sequence, not a row count: sequences ignore transaction visibility, so
    // the 1001st insert trips regardless of which chunk the hasher put a txid
    // in. `distinct_txids` iterates a HashSet, so the order is not ours to pick.
    {
        let mut conn = pool.get().await.expect("checkout connection");
        diesel::sql_query("CREATE SEQUENCE chunk_atomicity_counter")
            .execute(&mut conn)
            .await
            .expect("create sequence");
        diesel::sql_query(format!(
            "CREATE FUNCTION chunk_atomicity_guard() RETURNS trigger AS $$ \
             BEGIN \
               IF nextval('chunk_atomicity_counter') > {TRANSACTION_INSERT_CHUNK_SIZE} THEN \
                 RAISE EXCEPTION 'second chunk rejected'; \
               END IF; \
               RETURN NEW; \
             END $$ LANGUAGE plpgsql"
        ))
        .execute(&mut conn)
        .await
        .expect("create guard function");
        diesel::sql_query(
            "CREATE TRIGGER chunk_atomicity_trigger BEFORE INSERT ON transactions \
             FOR EACH ROW WHEN (NEW.txid LIKE 'chunk-atomicity-%') \
             EXECUTE FUNCTION chunk_atomicity_guard()",
        )
        .execute(&mut conn)
        .await
        .expect("create trigger");
    }

    let result = repo.write_batch(&entries).await;

    let mut conn = pool.get().await.expect("checkout connection");
    let tx_count: i64 = transactions::table
        .filter(transactions::txid.like("chunk-atomicity-%"))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count transactions");
    let delta_count: i64 = mempool_deltas::table
        .filter(mempool_deltas::txid.like("chunk-atomicity-%"))
        .count()
        .get_result(&mut conn)
        .await
        .expect("count deltas");

    // Tear down before asserting: on a real pool a failed assertion would
    // otherwise leave the trigger armed for whatever runs in this slot next.
    for stmt in [
        "DROP TRIGGER chunk_atomicity_trigger ON transactions",
        "DROP FUNCTION chunk_atomicity_guard",
        "DROP SEQUENCE chunk_atomicity_counter",
    ] {
        diesel::sql_query(stmt)
            .execute(&mut conn)
            .await
            .expect("tear down guard");
    }
    diesel::delete(transactions::table.filter(transactions::txid.like("chunk-atomicity-%")))
        .execute(&mut conn)
        .await
        .expect("clean up transactions");
    diesel::delete(mempool_deltas::table.filter(mempool_deltas::txid.like("chunk-atomicity-%")))
        .execute(&mut conn)
        .await
        .expect("clean up deltas");

    assert!(result.is_err(), "the guarded insert must surface its error");
    assert_eq!(
        tx_count, 0,
        "a failure in the second chunk must roll back the rows the first one wrote"
    );
    assert_eq!(
        delta_count, 0,
        "and the mempool_deltas rows written before it, or the log claims txids no \
         transactions row backs"
    );
}
