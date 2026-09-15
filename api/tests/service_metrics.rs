use api::infra::config::ApiConfig;
use api::services::cluster::ClusterService;
use api::services::cluster_delta::ClusterDeltaService;
use api::services::gauge_sample::GaugeSampleService;
use api::services::home::HomeService;
use shared::events::ClusterRef;
use testkit::deps::{inert_clients, inert_deps};
use testkit::fixtures::ClusterRefFixture;
use testkit::metrics::{assert_no_series, assert_series, capture};

fn cluster_service() -> ClusterService {
    inert_deps().cluster_service()
}

fn cluster_delta_service() -> ClusterDeltaService {
    inert_deps().cluster_delta_service()
}

fn home_service() -> HomeService {
    inert_deps().home_service()
}

/// A snapshot service seeded with the given active clusters and live mempool
/// txids, over an inert pool -- `sample()`'s insert fails, but the gauges it
/// sets along the way still fire.
fn gauge_sample_service(clusters: Vec<ClusterRef>, mempool_txids: &[&str]) -> GaugeSampleService {
    let deps = inert_deps();
    deps.cluster_snapshot.seed(clusters);
    deps.mempool_ledger
        .seed(mempool_txids.iter().map(|s| s.to_string()).collect());
    deps.gauge_sample_service()
}

fn cluster_ref(id: i64) -> ClusterRef {
    ClusterRefFixture::new(id).build()
}

// region: mempool_persist_failed_total

#[test]
fn persist_failed_total_counts_write_batch_failures_on_a_dead_pool() {
    let rendered = capture(async {
        let deps = inert_deps();
        deps.mempool_ledger.assert_present(&["a".to_string()]);
        deps.mempool_reconciler().tick().await;
    });
    assert_series(
        &rendered,
        r#"mempool_persist_failed_total{stage="write_batch"} 1"#,
    );
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
        let deps = inert_deps();
        let feerate_diagram_snapshot = deps.feerate_diagram_snapshot.clone();
        let state = deps.app_state();
        state
            .bootstrap_service
            .run(
                &ApiConfig::default(),
                inert_clients(),
                feerate_diagram_snapshot,
            )
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

// region: cluster_stale_member_count

#[test]
fn stale_member_count_counts_members_missing_from_the_live_mempool() {
    let rendered = capture(async {
        // cluster_ref(1) carries txids "a" and "b"; only "a" is live.
        gauge_sample_service(vec![cluster_ref(1)], &["a"])
            .sample()
            .await;
    });
    assert_series(&rendered, "cluster_stale_member_count 1");
}

#[test]
fn stale_member_count_is_zero_when_all_members_are_live() {
    let rendered = capture(async {
        gauge_sample_service(vec![cluster_ref(1)], &["a", "b"])
            .sample()
            .await;
    });
    assert_series(&rendered, "cluster_stale_member_count 0");
}

// endregion
