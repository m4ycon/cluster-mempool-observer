use api::db::instrument::sample_pool;
use api::db::models::{NewBlock, NewCluster, NewTransaction};
use api::db::{
    BlockRepository, ClusterMembershipRepository, ClusterMembershipUpdate, ClusterRepository,
    MempoolDeltaRepository, TransactionRepository,
};
use testkit::fixtures::MempoolDeltaFixture;
use testkit::metrics::{assert_no_series, assert_series, capture};
use testkit::postgres::inert_pool;
use time::OffsetDateTime;

fn expect_acquire_error(rendered: &str, repo: &str, op: &str) {
    assert_series(
        rendered,
        &format!(r#"db_pool_acquire_errors_total{{repo="{repo}",op="{op}"}} 1"#),
    );
}

#[test]
fn failed_checkout_is_still_timed() {
    let rendered = capture(async { BlockRepository::new(inert_pool()).latest_height().await });
    assert_series(&rendered, "db_pool_acquire_seconds_count 1");
}

#[test]
fn failed_checkout_increments_its_error_counter() {
    let rendered = capture(async { BlockRepository::new(inert_pool()).latest_height().await });
    expect_acquire_error(&rendered, "block", "latest_height");
}

/// No connection means no query ran, so claiming a query duration would be a
/// lie -- and would drag the query histogram down with fast failures.
#[test]
fn failed_checkout_records_no_query_duration() {
    let rendered = capture(async { BlockRepository::new(inert_pool()).latest_height().await });
    assert_no_series(&rendered, "db_query_seconds");
    assert_no_series(&rendered, "db_query_errors_total");
}

#[test]
fn pool_sampler_publishes_every_state() {
    let rendered = capture(async {
        sample_pool(&inert_pool());
    });
    assert_series(&rendered, r#"db_pool_connections{state="size"} 0"#);
    assert_series(&rendered, r#"db_pool_connections{state="available"} 0"#);
    assert_series(&rendered, r#"db_pool_connections{state="waiting"} 0"#);
}

/// The remaining tests pin the `repo` and `op` labels of every repository
/// method. These strings are a dashboard contract, and a typo in one is
/// invisible until a panel silently goes blank.
#[test]
fn block_repository_labels_every_call_site() {
    let rendered = capture(async {
        let repo = BlockRepository::new(inert_pool());
        let _ = repo
            .insert(&NewBlock {
                hash: "h".into(),
                height: 1,
                mined_at: OffsetDateTime::UNIX_EPOCH,
                tx_count: 0,
                total_bytes: 0,
                total_fee: 0,
                difficulty: 0.0,
            })
            .await;
        let _ = repo.latest_height().await;
        let _ = repo.latest().await;
    });

    for op in ["insert", "latest_height", "latest"] {
        expect_acquire_error(&rendered, "block", op);
    }
}

#[test]
fn cluster_repository_labels_every_call_site() {
    let rendered = capture(async {
        let repo = ClusterRepository::new(inert_pool());
        let new = NewCluster {
            txids: vec!["a".into(), "b".into()],
            total_vsize: 1,
            total_fee: 1,
            first_seen_at: None,
        };
        let _ = repo.insert(&new).await;
        let _ = repo.find_by_txid("a").await;
        let _ = repo.update(1, &["a".into()], 1, 1).await;
        let _ = repo.find_by_ids(&[1]).await;
        let _ = repo.find_active().await;
        let _ = repo.count().await;
        let _ = repo.find_active_ids_by_txids(&["a".into()]).await;
    });

    for op in [
        "insert",
        "find_by_txid",
        "update",
        "find_by_ids",
        "find_active",
        "count",
        "find_active_ids_by_txids",
    ] {
        expect_acquire_error(&rendered, "cluster", op);
    }
}

#[test]
fn transaction_repository_labels_every_call_site() {
    let rendered = capture(async {
        let repo = TransactionRepository::new(inert_pool());
        let txids = vec!["a".to_string()];
        let _ = repo.existing_txids(&txids).await;
        let _ = repo.insert(&NewTransaction::hollow("a")).await;
        let _ = repo
            .insert_or_confirm_many(&[NewTransaction::hollow("a")])
            .await;
        let _ = repo.get_fee_vsize_totals(&txids).await;
        let _ = repo.get_cluster_ids_by_txids(&txids).await;
        let _ = repo.set_cluster_id(&txids, 1).await;
    });

    for op in [
        "existing_txids",
        "insert",
        "insert_or_confirm_many",
        "get_fee_vsize_totals",
        "get_cluster_ids_by_txids",
        "set_cluster_id",
    ] {
        expect_acquire_error(&rendered, "transaction", op);
    }
}

#[test]
fn mempool_delta_repository_labels_every_call_site() {
    let rendered = capture(async {
        let repo = MempoolDeltaRepository::new(inert_pool());
        let _ = repo
            .insert_many(&[MempoolDeltaFixture::added("a").build()])
            .await;
        let _ = repo.record_removes_for_unpaired(&["a".to_string()]).await;
        let _ = repo.count().await;
        let _ = repo.count_adds_since(OffsetDateTime::UNIX_EPOCH).await;
        let _ = repo.reconstruct_snapshot().await;
    });

    for op in [
        "insert_many",
        "record_removes_for_unpaired",
        "count",
        "count_adds_since",
        "reconstruct_snapshot",
    ] {
        expect_acquire_error(&rendered, "mempool_delta", op);
    }
}

#[test]
fn cluster_membership_repository_labels_every_call_site() {
    let rendered = capture(async {
        let repo = ClusterMembershipRepository::new(inert_pool());
        let members = vec!["a".to_string(), "b".to_string()];
        let _ = repo
            .insert_with_members(&NewCluster {
                txids: members.clone(),
                total_vsize: 1,
                total_fee: 1,
                first_seen_at: None,
            })
            .await;
        let _ = repo
            .replace_members(ClusterMembershipUpdate {
                cluster_id: 1,
                current_members: &members,
                total_vsize: 1,
                total_fee: 1,
            })
            .await;
        let _ = repo.close_many(&[1]).await;
        let _ = repo.confirm(1, OffsetDateTime::UNIX_EPOCH).await;
    });

    for op in [
        "insert_with_members",
        "replace_members",
        "close_many",
        "confirm",
    ] {
        expect_acquire_error(&rendered, "cluster_membership", op);
    }
}
