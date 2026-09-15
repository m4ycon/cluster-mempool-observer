#![cfg(feature = "db_integration_tests")]

use api::db::models::MempoolGaugeSampleRow;
use api::db::schema::mempool_gauge_samples;
use api::db::{DbPool, GaugeSampleRepository};
use api::infra::router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde_json::Value;
use std::collections::HashSet;
use testkit::deps::{deps, gauge_sample_service, inert_deps};
use testkit::fixtures::{ClusterRefFixture, NewMempoolGaugeSampleRowFixture, fixed_time};
use testkit::postgres::isolated_pool;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;

async fn find_row(pool: &DbPool, sampled_at: OffsetDateTime) -> Option<MempoolGaugeSampleRow> {
    let mut conn = pool.get().await.expect("conn");
    mempool_gauge_samples::table
        .find(sampled_at)
        .select(MempoolGaugeSampleRow::as_select())
        .first(&mut conn)
        .await
        .optional()
        .expect("load snapshot row")
}

async fn get(app: &axum::Router, path: &str) -> (StatusCode, Value) {
    let request = Request::builder().uri(path).body(Body::empty()).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

async fn get_text(app: &axum::Router, path: &str) -> (StatusCode, String) {
    let request = Request::builder().uri(path).body(Body::empty()).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn only_row(pool: &DbPool) -> MempoolGaugeSampleRow {
    let mut conn = pool.get().await.expect("conn");
    mempool_gauge_samples::table
        .select(MempoolGaugeSampleRow::as_select())
        .get_result(&mut conn)
        .await
        .expect("exactly one snapshot row")
}

#[tokio::test]
async fn sample_persists_the_live_in_memory_state() {
    let pool = isolated_pool().await;
    let service = gauge_sample_service(
        pool.clone(),
        vec![
            ClusterRefFixture::new(1)
                .with_txids(&["a", "b"])
                .with_total_vsize(200)
                .with_total_fee(900)
                .build(),
            ClusterRefFixture::new(2)
                .with_txids(&["c"])
                .with_total_vsize(100)
                .with_total_fee(300)
                .build(),
        ],
        HashSet::from([
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ]),
    );

    service.sample().await;

    let stored = only_row(&pool).await;
    assert_eq!(stored.cluster_count, 2);
    assert_eq!(stored.clustered_tx_count, 3, "a, b, c across both clusters");
    assert_eq!(
        stored.mempool_tx_count, 4,
        "d is in the mempool but unclustered"
    );
    assert_eq!(stored.total_vsize, 300);
    assert_eq!(stored.total_fee, 1_200);
}

#[tokio::test]
async fn insert_persists_a_row() {
    let pool = isolated_pool().await;
    let repo = GaugeSampleRepository::new(pool.clone());

    let when = fixed_time();
    let row = NewMempoolGaugeSampleRowFixture::new(when)
        .with_cluster_count(3)
        .with_clustered_tx_count(7)
        .with_mempool_tx_count(9)
        .with_total_vsize(1_234)
        .with_total_fee(5_678)
        .build();

    let affected = repo.insert(&row).await.expect("insert gauge sample");
    assert_eq!(affected, 1);

    let stored = find_row(&pool, when).await.expect("row exists");
    assert_eq!(stored.cluster_count, 3);
    assert_eq!(stored.clustered_tx_count, 7);
    assert_eq!(stored.mempool_tx_count, 9);
    assert_eq!(stored.total_vsize, 1_234);
    assert_eq!(stored.total_fee, 5_678);
}

#[tokio::test]
async fn insert_on_conflicting_sampled_at_does_nothing() {
    let pool = isolated_pool().await;
    let repo = GaugeSampleRepository::new(pool.clone());

    let when = fixed_time();
    let first = NewMempoolGaugeSampleRowFixture::new(when)
        .with_cluster_count(1)
        .build();
    repo.insert(&first).await.expect("insert first");

    // A second sample landing on the exact same instant must not error or
    // overwrite: collisions are near-impossible but must never crash the loop.
    let second = NewMempoolGaugeSampleRowFixture::new(when)
        .with_cluster_count(99)
        .build();
    let affected = repo.insert(&second).await.expect("insert does not error");
    assert_eq!(affected, 0, "conflicting row is not applied");

    let stored = find_row(&pool, when).await.expect("row exists");
    assert_eq!(stored.cluster_count, 1, "original row is unchanged");
}

#[tokio::test]
async fn range_at_native_resolution_returns_every_row_uncollapsed() {
    let pool = isolated_pool().await;
    let repo = GaugeSampleRepository::new(pool);

    let t0 = fixed_time();
    let t1 = t0 + Duration::seconds(10); // close enough to fall in the same bucket if bucketed
    let t2 = t0 + Duration::seconds(70);

    for (when, cluster_count) in [(t0, 1), (t1, 2), (t2, 3)] {
        let row = NewMempoolGaugeSampleRowFixture::new(when)
            .with_cluster_count(cluster_count)
            .build();
        repo.insert(&row).await.expect("insert gauge sample");
    }

    let points = repo.range(t0, t2, 60).await.expect("range query");

    assert_eq!(
        points.len(),
        3,
        "native resolution must return every row, not collapse close samples"
    );
    assert_eq!(points[0].sampled_at.unix_timestamp(), t0.unix_timestamp());
    assert_eq!(points[0].cluster_count, 1);
    assert_eq!(points[1].sampled_at.unix_timestamp(), t1.unix_timestamp());
    assert_eq!(points[1].cluster_count, 2);
    assert_eq!(points[2].sampled_at.unix_timestamp(), t2.unix_timestamp());
    assert_eq!(points[2].cluster_count, 3);
}

#[tokio::test]
async fn range_bucketed_returns_the_last_row_per_bucket_not_an_average() {
    let pool = isolated_pool().await;
    let repo = GaugeSampleRepository::new(pool);

    // Aligned to a 300s boundary so bucket membership is unambiguous.
    let bucket_a_start = OffsetDateTime::from_unix_timestamp(1_700_000_400).unwrap();
    let bucket_b_start = bucket_a_start + Duration::seconds(300);

    let a1 = bucket_a_start;
    let a2 = bucket_a_start + Duration::seconds(50);
    let a3 = bucket_a_start + Duration::seconds(250); // last (real) row of bucket A
    let b1 = bucket_b_start;
    let b2 = bucket_b_start + Duration::seconds(60); // last (real) row of bucket B

    // (when, cluster_count, clustered_tx_count, mempool_tx_count, total_vsize, total_fee)
    let rows: [(OffsetDateTime, i32, i32, i32, i64, i64); 5] = [
        (a1, 1, 10, 100, 1_000, 10_000),
        (a2, 2, 20, 200, 2_000, 20_000),
        (a3, 3, 30, 300, 3_000, 30_000),
        (b1, 10, 100, 1_000, 10_000, 100_000),
        (b2, 20, 200, 2_000, 20_000, 200_000),
    ];
    for (when, cluster_count, clustered_tx_count, mempool_tx_count, total_vsize, total_fee) in rows
    {
        let row = NewMempoolGaugeSampleRowFixture::new(when)
            .with_cluster_count(cluster_count)
            .with_clustered_tx_count(clustered_tx_count)
            .with_mempool_tx_count(mempool_tx_count)
            .with_total_vsize(total_vsize)
            .with_total_fee(total_fee)
            .build();
        repo.insert(&row).await.expect("insert gauge sample");
    }

    let points = repo
        .range(bucket_a_start, b2, 300)
        .await
        .expect("range query");

    assert_eq!(points.len(), 2, "one row per bucket, not one per insert");

    // Bucket A: last row's cluster_count is 3; average of {1,2,3} is 2, first is 1.
    assert_eq!(
        points[0].sampled_at.unix_timestamp(),
        a3.unix_timestamp(),
        "sampled_at must be the real last-row timestamp, not a synthetic bucket boundary"
    );
    assert_ne!(
        points[0].sampled_at.unix_timestamp(),
        bucket_a_start.unix_timestamp()
    );
    assert_eq!(points[0].cluster_count, 3);
    assert_eq!(points[0].clustered_tx_count, 30);
    assert_eq!(points[0].mempool_tx_count, 300);
    assert_eq!(points[0].total_vsize, 3_000);
    assert_eq!(points[0].total_fee, 30_000);

    // Bucket B: last row's cluster_count is 20; average of {10,20} is 15, first is 10.
    assert_eq!(points[1].sampled_at.unix_timestamp(), b2.unix_timestamp());
    assert_eq!(points[1].cluster_count, 20);
    assert_eq!(points[1].clustered_tx_count, 200);
    assert_eq!(points[1].mempool_tx_count, 2_000);
    assert_eq!(points[1].total_vsize, 20_000);
    assert_eq!(points[1].total_fee, 200_000);
}

#[tokio::test]
async fn metric_route_projects_each_variant_to_its_own_column() {
    let pool = isolated_pool().await;
    let repo = GaugeSampleRepository::new(pool.clone());

    // now_utc() so the default (no from/to) 24h window covers it.
    let when = OffsetDateTime::now_utc();
    let row = NewMempoolGaugeSampleRowFixture::new(when)
        .with_cluster_count(11)
        .with_clustered_tx_count(22)
        .with_mempool_tx_count(33)
        .with_total_vsize(444)
        .with_total_fee(555)
        .build();
    repo.insert(&row).await.expect("insert gauge sample");

    let app = router::build(deps(pool).app_state());

    // distinct value per column: a copy-paste bug mapping two variants to the
    // same column fails one of these.
    let cases: [(&str, i64); 5] = [
        ("cluster-count", 11),
        ("clustered-tx-count", 22),
        ("mempool-tx-count", 33),
        ("total-vsize", 444),
        ("total-fee", 555),
    ];

    for (metric, expected) in cases {
        let (status, body) = get(&app, &format!("/mempool/gauges/{metric}")).await;
        assert_eq!(status, StatusCode::OK, "metric {metric}");
        assert_eq!(body["metric"], metric);
        assert_eq!(body["points"][0]["value"], expected, "metric {metric}");
    }
}

#[tokio::test]
async fn metric_route_bucketing_keeps_the_last_real_row_per_bucket() {
    let pool = isolated_pool().await;
    let repo = GaugeSampleRepository::new(pool.clone());

    // Same bucket layout as `range_bucketed_returns_the_last_row_per_bucket_not_an_average`.
    let bucket_a_start = OffsetDateTime::from_unix_timestamp(1_700_000_400).unwrap();
    let bucket_b_start = bucket_a_start + Duration::seconds(300);

    let rows: [(OffsetDateTime, i32); 5] = [
        (bucket_a_start, 1),
        (bucket_a_start + Duration::seconds(50), 2),
        (bucket_a_start + Duration::seconds(250), 3), // last real row of bucket A
        (bucket_b_start, 10),
        (bucket_b_start + Duration::seconds(60), 20), // last real row of bucket B
    ];
    for (when, cluster_count) in rows {
        let row = NewMempoolGaugeSampleRowFixture::new(when)
            .with_cluster_count(cluster_count)
            .build();
        repo.insert(&row).await.expect("insert gauge sample");
    }

    let app = router::build(deps(pool).app_state());

    let from = bucket_a_start.format(&Rfc3339).unwrap();
    let to = (bucket_a_start + Duration::hours(40))
        .format(&Rfc3339)
        .unwrap(); // forces the 300s rung
    let uri = format!("/mempool/gauges/cluster-count?from={from}&to={to}");

    let (status, body) = get(&app, &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["resolution_secs"], 300);

    let points = body["points"].as_array().expect("points array");
    assert_eq!(points.len(), 2, "one point per bucket, not one per insert");
    assert_eq!(
        points[0]["value"], 3,
        "bucket A keeps the last real row, not the first or an average"
    );
    assert_eq!(points[1]["value"], 20, "bucket B keeps the last real row");
}

#[tokio::test]
async fn metric_route_rejects_an_unknown_metric_with_bad_request() {
    let app = router::build(inert_deps().app_state());
    let (status, _) = get(&app, "/mempool/gauges/not_a_real_metric").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn metric_route_rejects_an_invalid_range_with_bad_request() {
    let app = router::build(inert_deps().app_state());
    let (status, _) = get(
        &app,
        "/mempool/gauges/cluster-count?from=2024-01-02T00:00:00Z&to=2024-01-01T00:00:00Z",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn metric_route_surfaces_a_repository_failure_as_internal_server_error() {
    let app = router::build(inert_deps().app_state());
    let (status, body) = get_text(&app, "/mempool/gauges/cluster-count").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    // the inert pool's DSN (host, user, pass) must never reach the client.
    assert!(
        !body.contains("127.0.0.1") && !body.contains("user") && !body.contains("pass"),
        "response body must not leak the underlying repository error: {body}"
    );
}
