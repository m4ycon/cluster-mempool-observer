use api::db::{
    BlockRepository, ClusterMembershipRepository, ClusterRepository, DbPool,
    MempoolDeltaRepository, TransactionRepository, build_pool,
};
use api::infra::config::ApiConfig;
use api::infra::state::AppState;
use api::services::cluster::ClusterService;
use api::services::cluster_delta::ClusterDeltaService;
use api::services::home::HomeService;
use api::services::mempool::MempoolService;
use api::services::pubsub::PubSubService;
use observer::clients::rpc_client::RpcClient;
use observer::infra::config::{Config as ObserverConfig, RpcConfig};
use observer::retrievers::{ClusterRpcRetriever, MempoolRetriever, TransactionRpcRetriever};
use shared::events::{ClusterRef, MempoolDeltaEvent};
use shared::pubsub::PubSub;
use shared::snapshot::{ClusterSnapshot, MempoolSnapshot};
use testkit::metrics::{assert_no_series, assert_series, capture};

/// Dependencies that fail fast: a database that will not answer and a closed
/// RPC port.
///
/// Instrumentation has to record regardless of outcome -- a stage that errors
/// is exactly the one whose latency matters -- so unreachable dependencies
/// still drive every timer under test.
fn inert_pool() -> DbPool {
    build_pool("postgres://user:pass@127.0.0.1:1/nothing").expect("pool builds lazily")
}

fn inert_rpc() -> RpcClient {
    RpcClient::new(&RpcConfig {
        host: "127.0.0.1:1".into(),
        user: String::new(),
        pass: String::new(),
    })
    .expect("rpc client builds without connecting")
}

fn cluster_service() -> ClusterService {
    let pool = inert_pool();
    ClusterService::new(
        ClusterRepository::new(pool.clone()),
        TransactionRepository::new(pool.clone()),
        ClusterMembershipRepository::new(pool),
        ClusterRpcRetriever::new(inert_rpc()),
        cluster_delta_service(),
    )
}

fn cluster_delta_service() -> ClusterDeltaService {
    ClusterDeltaService::new(
        ClusterSnapshot::default(),
        PubSubService::new(PubSub::new()),
    )
}

fn mempool_service() -> MempoolService {
    let pool = inert_pool();
    MempoolService::new(
        MempoolDeltaRepository::new(pool.clone()),
        TransactionRepository::new(pool),
        TransactionRpcRetriever::new(inert_rpc()),
        cluster_service(),
        PubSubService::new(PubSub::new()),
    )
}

fn home_service() -> HomeService {
    let pool = inert_pool();
    HomeService::new(
        BlockRepository::new(pool.clone()),
        MempoolDeltaRepository::new(pool),
        MempoolRetriever::new(inert_rpc(), MempoolSnapshot::default()),
        cluster_service(),
        PubSubService::new(PubSub::new()),
    )
}

fn delta(added: &[&str], removed: &[&str]) -> MempoolDeltaEvent {
    MempoolDeltaEvent {
        added: added.iter().map(|s| s.to_string()).collect(),
        removed: removed.iter().map(|s| s.to_string()).collect(),
    }
}

fn cluster_ref(id: i64) -> ClusterRef {
    ClusterRef {
        id,
        txids: vec!["a".into(), "b".into()],
        total_vsize: 100,
        total_fee: 200,
    }
}

// region: mempool_delta_apply_seconds

#[test]
fn apply_seconds_records_once_per_delta() {
    let rendered = capture(async {
        mempool_service().apply_delta(delta(&["a"], &[])).await;
    });
    assert_series(&rendered, "mempool_delta_apply_seconds_count 1");
}

// endregion

// region: mempool_delta_txs_total

#[test]
fn txs_total_counts_added_txids() {
    let rendered = capture(async {
        mempool_service()
            .apply_delta(delta(&["a", "b", "c"], &[]))
            .await;
    });
    assert_series(&rendered, r#"mempool_delta_txs_total{direction="added"} 3"#);
}

#[test]
fn txs_total_counts_removed_txids() {
    let rendered = capture(async {
        mempool_service().apply_delta(delta(&[], &["a", "b"])).await;
    });
    assert_series(
        &rendered,
        r#"mempool_delta_txs_total{direction="removed"} 2"#,
    );
}

#[test]
fn txs_total_accumulates_across_deltas() {
    let rendered = capture(async {
        let service = mempool_service();
        service.apply_delta(delta(&["a"], &[])).await;
        service.apply_delta(delta(&["b", "c"], &[])).await;
    });
    assert_series(&rendered, r#"mempool_delta_txs_total{direction="added"} 3"#);
    assert_series(&rendered, "mempool_delta_apply_seconds_count 2");
}

// endregion

// region: cluster_sync_seconds

#[test]
fn sync_seconds_records_the_evicted_stage() {
    let rendered = capture(async {
        cluster_service()
            .sync_clusters_for(&[], &["a".into()])
            .await;
    });
    assert_series(
        &rendered,
        r#"cluster_sync_seconds_count{stage="evicted"} 1"#,
    );
}

#[test]
fn sync_seconds_records_the_candidates_stage() {
    let rendered = capture(async {
        cluster_service()
            .sync_clusters_for(&["a".into()], &[])
            .await;
    });
    assert_series(
        &rendered,
        r#"cluster_sync_seconds_count{stage="candidates"} 1"#,
    );
}

#[test]
fn sync_seconds_skips_stages_with_no_input() {
    let rendered = capture(async {
        cluster_service()
            .sync_clusters_for(&["a".into()], &[])
            .await;
    });
    assert_no_series(&rendered, r#"cluster_sync_seconds_count{stage="evicted"}"#);
}

#[test]
fn sync_seconds_records_nothing_on_an_empty_round() {
    let rendered = capture(async {
        cluster_service().sync_clusters_for(&[], &[]).await;
    });
    assert_no_series(&rendered, "cluster_sync_seconds");
}

#[test]
fn sync_seconds_skips_publish_without_changes() {
    let rendered = capture(async {
        cluster_service()
            .sync_clusters_for(&["a".into()], &[])
            .await;
    });
    assert_no_series(&rendered, r#"cluster_sync_seconds_count{stage="publish"}"#);
}

// endregion

// region: cluster_confirm_mined_seconds

#[test]
fn confirm_mined_seconds_records_both_stages() {
    let rendered = capture(async {
        cluster_service()
            .confirm_mined(
                &["a".into()],
                &Default::default(),
                &Default::default(),
                time::OffsetDateTime::UNIX_EPOCH,
            )
            .await;
    });
    for stage in ["reconcile", "publish"] {
        assert_series(
            &rendered,
            &format!(r#"cluster_confirm_mined_seconds_count{{stage="{stage}"}} 1"#),
        );
    }
}

// endregion

// region: cluster_delta_published_total

#[test]
fn published_total_counts_upserted_clusters() {
    let rendered = capture(async {
        cluster_delta_service()
            .publish([cluster_ref(1), cluster_ref(2)], [])
            .await;
    });
    assert_series(
        &rendered,
        r#"cluster_delta_published_total{kind="upserted"} 2"#,
    );
}

#[test]
fn published_total_counts_removed_clusters() {
    let rendered = capture(async {
        let service = cluster_delta_service();
        service.seed([cluster_ref(1)]);
        service.publish([], [1]).await;
    });
    assert_series(
        &rendered,
        r#"cluster_delta_published_total{kind="removed"} 1"#,
    );
}

#[test]
fn published_total_ignores_a_no_op_round() {
    let rendered = capture(async {
        cluster_delta_service().publish([], []).await;
    });
    assert_no_series(&rendered, "cluster_delta_published_total");
}

// endregion

// region: cluster_active_count

#[test]
fn active_count_tracks_the_snapshot() {
    let rendered = capture(async {
        cluster_delta_service()
            .publish([cluster_ref(1), cluster_ref(2)], [])
            .await;
    });
    assert_series(&rendered, "cluster_active_count 2");
}

#[test]
fn active_count_is_published_on_a_no_op_round() {
    let rendered = capture(async {
        cluster_delta_service().publish([], []).await;
    });
    assert_series(&rendered, "cluster_active_count 0");
}

// endregion

// region: cluster_snapshot_build_seconds

#[test]
fn snapshot_build_seconds_records_per_subscriber() {
    let rendered = capture(async {
        let service = cluster_service();
        let _ = service.get_current_snapshot();
        let _ = service.get_current_snapshot();
    });
    assert_series(&rendered, "cluster_snapshot_build_seconds_count 2");
}

// endregion

// region: home_stats_seconds

#[test]
fn home_stats_seconds_records_per_tick() {
    let rendered = capture(async {
        let _ = home_service().current_stats().await;
    });
    assert_series(&rendered, "home_stats_seconds_count 1");
}

// endregion

// region: bootstrap_stage_seconds

#[test]
fn bootstrap_stage_seconds_records_every_startup_stage() {
    let rendered = capture(async {
        let config = ObserverConfig {
            rpc: RpcConfig {
                host: "127.0.0.1:1".into(),
                user: String::new(),
                pass: String::new(),
            },
            ..Default::default()
        };
        let (state, snapshot, clients) = AppState::build(&config, inert_pool());
        state
            .bootstrap_service
            .run(&ApiConfig::default(), clients, snapshot)
            .await;
    });

    for stage in ["sync_missing_blocks", "mempool_snapshot", "seed_clusters"] {
        assert_series(
            &rendered,
            &format!(r#"bootstrap_stage_seconds_count{{stage="{stage}"}} 1"#),
        );
    }
}

// endregion
