#![cfg(feature = "db_integration_tests")]

use api::db::models::{DeltaReason, MempoolCounterSampleRow, SystemEventKind};
use api::db::schema::{mempool_deltas, system_events};
use api::db::{CounterSampleRepository, DbPool, MempoolDeltaRepository, SystemEventRepository};
use api::infra::router;
use api::services::counter_sample::CounterSampleService;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde_json::Value;
use std::time::Duration as StdDuration;
use testkit::deps::{deps, inert_deps};
use testkit::fixtures::{
    MempoolDeltaFixture, NewMempoolCounterSampleRowFixture, NewSystemEventFixture,
};
use testkit::postgres::isolated_pool;
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;

/// Overrides the DB-set `created_at` so window-boundary cases can be exercised.
async fn backdate_delta(pool: &DbPool, txid: &str, at: OffsetDateTime) {
    let mut conn = pool.get().await.expect("conn");
    diesel::update(mempool_deltas::table.filter(mempool_deltas::txid.eq(txid)))
        .set(mempool_deltas::created_at.eq(at))
        .execute(&mut conn)
        .await
        .expect("backdate created_at");
}

/// Same, for the one `system_events` row a test just inserted.
async fn backdate_latest_system_event(pool: &DbPool, at: OffsetDateTime) {
    let mut conn = pool.get().await.expect("conn");
    let id: i64 = system_events::table
        .select(system_events::id)
        .order(system_events::id.desc())
        .first(&mut conn)
        .await
        .expect("latest system event id");
    diesel::update(system_events::table.filter(system_events::id.eq(id)))
        .set(system_events::created_at.eq(at))
        .execute(&mut conn)
        .await
        .expect("backdate created_at");
}

async fn find_row(pool: &DbPool, sampled_at: OffsetDateTime) -> Option<MempoolCounterSampleRow> {
    use api::db::schema::mempool_counter_samples;
    let mut conn = pool.get().await.expect("conn");
    mempool_counter_samples::table
        .find(sampled_at)
        .select(MempoolCounterSampleRow::as_select())
        .first(&mut conn)
        .await
        .optional()
        .expect("load counter sample row")
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

#[tokio::test]
async fn count_deltas_in_excludes_start_includes_end_and_splits_by_reason() {
    let pool = isolated_pool().await;
    let delta_repo = MempoolDeltaRepository::new(pool.clone());

    delta_repo
        .insert_many(&[
            MempoolDeltaFixture::added("at_start").build(),
            MempoolDeltaFixture::added("at_end").build(),
            MempoolDeltaFixture::added("inside").build(),
            MempoolDeltaFixture::new("confirmed", DeltaReason::RemoveConfirmed).build(),
            MempoolDeltaFixture::new("evicted", DeltaReason::RemoveEvicted).build(),
        ])
        .await
        .expect("insert deltas");

    let window_start = OffsetDateTime::now_utc() - Duration::seconds(120);
    let window_end = OffsetDateTime::now_utc() - Duration::seconds(60);
    let inside = window_start + Duration::seconds(30);

    backdate_delta(&pool, "at_start", window_start).await;
    backdate_delta(&pool, "at_end", window_end).await;
    backdate_delta(&pool, "inside", inside).await;
    backdate_delta(&pool, "confirmed", inside).await;
    backdate_delta(&pool, "evicted", inside).await;

    let (added, confirmed, evicted) = delta_repo
        .count_deltas_in(window_start, window_end)
        .await
        .expect("count deltas");

    assert_eq!(
        added, 2,
        "the delta exactly at window_start must be excluded, the one at window_end included"
    );
    assert_eq!(confirmed, 1);
    assert_eq!(evicted, 1);
}

#[tokio::test]
async fn sample_writes_a_null_row_on_the_first_ever_call() {
    let pool = isolated_pool().await;
    let counter_repo = CounterSampleRepository::new(pool.clone());
    let delta_repo = MempoolDeltaRepository::new(pool.clone());
    let system_event_repo = SystemEventRepository::new(pool.clone());
    let interval = StdDuration::from_secs(90);
    let service =
        CounterSampleService::new(counter_repo.clone(), delta_repo.clone(), system_event_repo);

    service.sample(interval).await;

    let sampled_at = counter_repo
        .latest_sampled_at()
        .await
        .expect("query cursor")
        .expect("first sample sets the cursor");
    let row = find_row(&pool, sampled_at).await.expect("row exists");

    assert_eq!(row.added_txs, None);
    assert_eq!(row.confirmed_txs, None);
    assert_eq!(row.evicted_txs, None);
    assert_eq!(
        row.period_secs, 90,
        "with no prior window, period_secs falls back to the configured interval"
    );
}

#[tokio::test]
async fn sample_counts_deltas_in_the_window_and_advances_the_cursor() {
    let pool = isolated_pool().await;
    let counter_repo = CounterSampleRepository::new(pool.clone());
    let delta_repo = MempoolDeltaRepository::new(pool.clone());
    let system_event_repo = SystemEventRepository::new(pool.clone());
    let interval = StdDuration::from_secs(60);
    let service =
        CounterSampleService::new(counter_repo.clone(), delta_repo.clone(), system_event_repo);

    let cursor_at = OffsetDateTime::now_utc() - Duration::seconds(120);
    counter_repo
        .insert(&NewMempoolCounterSampleRowFixture::new(cursor_at).build())
        .await
        .expect("seed cursor");

    delta_repo
        .insert_many(&[
            MempoolDeltaFixture::added("a").build(),
            MempoolDeltaFixture::added("b").build(),
            MempoolDeltaFixture::new("c", DeltaReason::RemoveConfirmed).build(),
        ])
        .await
        .expect("insert deltas");
    // Land the deltas well inside the window: `sample`'s safety margin leaves
    // the most recent 60s of `mempool_deltas` uncounted on purpose.
    let inside = cursor_at + Duration::seconds(30);
    backdate_delta(&pool, "a", inside).await;
    backdate_delta(&pool, "b", inside).await;
    backdate_delta(&pool, "c", inside).await;

    service.sample(interval).await;

    let sampled_at = counter_repo
        .latest_sampled_at()
        .await
        .expect("query cursor")
        .expect("cursor set");
    assert!(sampled_at > cursor_at, "cursor must advance");

    let row = find_row(&pool, sampled_at).await.expect("row exists");
    assert_eq!(row.added_txs, Some(2));
    assert_eq!(row.confirmed_txs, Some(1));
    assert_eq!(row.evicted_txs, Some(0));
}

#[tokio::test]
async fn sample_skips_counting_and_writes_null_when_a_restart_lands_in_the_window() {
    let pool = isolated_pool().await;
    let counter_repo = CounterSampleRepository::new(pool.clone());
    let delta_repo = MempoolDeltaRepository::new(pool.clone());
    let system_event_repo = SystemEventRepository::new(pool.clone());
    let interval = StdDuration::from_secs(60);
    let service = CounterSampleService::new(
        counter_repo.clone(),
        delta_repo.clone(),
        system_event_repo.clone(),
    );

    let cursor_at = OffsetDateTime::now_utc() - Duration::seconds(120);
    counter_repo
        .insert(&NewMempoolCounterSampleRowFixture::new(cursor_at).build())
        .await
        .expect("seed cursor");

    // A tx that would otherwise be counted as an arrival.
    delta_repo
        .insert_many(&[MempoolDeltaFixture::added("bootstrap_replay").build()])
        .await
        .expect("insert delta");
    let inside = cursor_at + Duration::seconds(30);
    backdate_delta(&pool, "bootstrap_replay", inside).await;

    // The restart that would have produced that false spike.
    system_event_repo
        .insert(&NewSystemEventFixture::new(SystemEventKind::ServerStarted).build())
        .await
        .expect("insert restart event");
    backdate_latest_system_event(&pool, inside).await;

    service.sample(interval).await;

    let sampled_at = counter_repo
        .latest_sampled_at()
        .await
        .expect("query cursor")
        .expect("cursor still advances so the spike is not swept into the next window");
    assert!(sampled_at > cursor_at);

    let row = find_row(&pool, sampled_at).await.expect("row exists");
    assert_eq!(
        row.added_txs, None,
        "counts stay null across a restart window"
    );
    assert_eq!(row.confirmed_txs, None);
    assert_eq!(row.evicted_txs, None);
}

#[tokio::test]
async fn sample_period_secs_reflects_a_skipped_tick_not_the_configured_interval() {
    let pool = isolated_pool().await;
    let counter_repo = CounterSampleRepository::new(pool.clone());
    let delta_repo = MempoolDeltaRepository::new(pool.clone());
    let system_event_repo = SystemEventRepository::new(pool.clone());
    let interval = StdDuration::from_secs(60);
    let service =
        CounterSampleService::new(counter_repo.clone(), delta_repo.clone(), system_event_repo);

    // A cursor far older than one interval simulates a stalled/skipped tick.
    let cursor_at = OffsetDateTime::now_utc() - Duration::seconds(300);
    counter_repo
        .insert(&NewMempoolCounterSampleRowFixture::new(cursor_at).build())
        .await
        .expect("seed cursor");

    service.sample(interval).await;

    let sampled_at = counter_repo
        .latest_sampled_at()
        .await
        .expect("query cursor")
        .expect("cursor set");
    let row = find_row(&pool, sampled_at).await.expect("row exists");

    assert!(
        row.period_secs > interval.as_secs() as i64,
        "a window this wide (~240s) must not be reported as the 60s configured interval"
    );
}

#[tokio::test]
async fn range_at_native_resolution_returns_every_row_uncollapsed() {
    let pool = isolated_pool().await;
    let repo = CounterSampleRepository::new(pool);

    let t0 = OffsetDateTime::from_unix_timestamp(1_700_000_400).unwrap();
    let t1 = t0 + Duration::seconds(10); // close enough to fall in the same bucket if bucketed
    let t2 = t0 + Duration::seconds(70);

    for (when, added) in [(t0, 1), (t1, 2), (t2, 3)] {
        let row = NewMempoolCounterSampleRowFixture::new(when)
            .with_added_txs(Some(added))
            .build();
        repo.insert(&row).await.expect("insert counter sample");
    }

    let points = repo.range(t0, t2, 60).await.expect("range query");

    assert_eq!(
        points.len(),
        3,
        "native resolution must return every row, not collapse close samples"
    );
    assert_eq!(points[0].sampled_at.unix_timestamp(), t0.unix_timestamp());
    assert_eq!(points[0].added_txs, Some(1));
    assert_eq!(points[1].sampled_at.unix_timestamp(), t1.unix_timestamp());
    assert_eq!(points[1].added_txs, Some(2));
    assert_eq!(points[2].sampled_at.unix_timestamp(), t2.unix_timestamp());
    assert_eq!(points[2].added_txs, Some(3));
}

#[tokio::test]
async fn range_bucketed_sums_across_rows_in_a_bucket_instead_of_keeping_the_last_one() {
    let pool = isolated_pool().await;
    let repo = CounterSampleRepository::new(pool);

    // Aligned to a 300s boundary so bucket membership is unambiguous.
    let bucket_a_start = OffsetDateTime::from_unix_timestamp(1_700_000_400).unwrap();
    let bucket_b_start = bucket_a_start + Duration::seconds(300);

    let a1 = bucket_a_start;
    let a2 = bucket_a_start + Duration::seconds(50);
    let a3 = bucket_a_start + Duration::seconds(250);
    let b1 = bucket_b_start;
    let b2 = bucket_b_start + Duration::seconds(60);

    // (when, added, confirmed, evicted)
    let rows: [(OffsetDateTime, i64, i64, i64); 5] = [
        (a1, 1, 10, 100),
        (a2, 2, 20, 200),
        (a3, 3, 30, 300),
        (b1, 5, 50, 500),
        (b2, 7, 70, 700),
    ];
    for (when, added, confirmed, evicted) in rows {
        let row = NewMempoolCounterSampleRowFixture::new(when)
            .with_added_txs(Some(added))
            .with_confirmed_txs(Some(confirmed))
            .with_evicted_txs(Some(evicted))
            .build();
        repo.insert(&row).await.expect("insert counter sample");
    }

    let points = repo
        .range(bucket_a_start, b2, 300)
        .await
        .expect("range query");

    assert_eq!(points.len(), 2, "one row per bucket, not one per insert");

    // Bucket A: sum of {1,2,3}=6, {10,20,30}=60, {100,200,300}=600. The last
    // row's own values (3, 30, 300) would be wrong here -- that is the gauge
    // table's DISTINCT ON behavior, not what a counter needs.
    assert_eq!(points[0].added_txs, Some(6));
    assert_eq!(points[0].confirmed_txs, Some(60));
    assert_eq!(points[0].evicted_txs, Some(600));

    // Bucket B: sum of {5,7}=12, {50,70}=120, {500,700}=1200.
    assert_eq!(points[1].added_txs, Some(12));
    assert_eq!(points[1].confirmed_txs, Some(120));
    assert_eq!(points[1].evicted_txs, Some(1_200));
}

#[tokio::test]
async fn range_bucketed_any_null_row_poisons_the_whole_bucket() {
    let pool = isolated_pool().await;
    let repo = CounterSampleRepository::new(pool);

    let bucket_start = OffsetDateTime::from_unix_timestamp(1_700_000_400).unwrap();
    let measured_1 = bucket_start;
    let unmeasured = bucket_start + Duration::seconds(60); // e.g. a window straddling a restart
    let measured_2 = bucket_start + Duration::seconds(120);

    repo.insert(
        &NewMempoolCounterSampleRowFixture::new(measured_1)
            .with_added_txs(Some(1))
            .with_confirmed_txs(Some(1))
            .with_evicted_txs(Some(1))
            .build(),
    )
    .await
    .expect("insert measured row");
    repo.insert(
        &NewMempoolCounterSampleRowFixture::new(unmeasured)
            .with_added_txs(None)
            .with_confirmed_txs(None)
            .with_evicted_txs(None)
            .build(),
    )
    .await
    .expect("insert unmeasured row");
    repo.insert(
        &NewMempoolCounterSampleRowFixture::new(measured_2)
            .with_added_txs(Some(2))
            .with_confirmed_txs(Some(2))
            .with_evicted_txs(Some(2))
            .build(),
    )
    .await
    .expect("insert measured row");

    let points = repo
        .range(bucket_start, measured_2, 300)
        .await
        .expect("range query");

    assert_eq!(points.len(), 1, "all three rows fall in the same bucket");
    assert_eq!(
        points[0].added_txs, None,
        "one unmeasured row must null the whole bucket, not just skip itself"
    );
    assert_eq!(points[0].confirmed_txs, None);
    assert_eq!(points[0].evicted_txs, None);
}

#[tokio::test]
async fn counters_route_returns_the_three_series_aligned_on_the_same_buckets() {
    let pool = isolated_pool().await;
    let repo = CounterSampleRepository::new(pool.clone());

    // now_utc() so the default (no from/to) 24h window covers it.
    let when = OffsetDateTime::now_utc();
    let row = NewMempoolCounterSampleRowFixture::new(when)
        .with_added_txs(Some(11))
        .with_confirmed_txs(Some(22))
        .with_evicted_txs(Some(33))
        .build();
    repo.insert(&row).await.expect("insert counter sample");

    let app = router::build(deps(pool).app_state());

    let (status, body) = get(&app, "/mempool/counters").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["resolution_secs"], 60);

    let points = body["points"].as_array().expect("points array");
    assert_eq!(points.len(), 1);
    // A single response carrying all three series on the same point is the
    // whole point of this endpoint: three separate requests could desync.
    assert_eq!(points[0]["added_txs"], 11);
    assert_eq!(points[0]["confirmed_txs"], 22);
    assert_eq!(points[0]["evicted_txs"], 33);
}

#[tokio::test]
async fn counters_route_rejects_a_from_not_earlier_than_to_with_bad_request() {
    let app = router::build(inert_deps().app_state());
    let (status, _) = get(
        &app,
        "/mempool/counters?from=2024-01-02T00:00:00Z&to=2024-01-01T00:00:00Z",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn counters_route_rejects_a_malformed_range_param_with_bad_request() {
    let app = router::build(inert_deps().app_state());
    let (status, _) = get(&app, "/mempool/counters?from=not-a-date").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn counters_route_surfaces_a_repository_failure_as_internal_server_error() {
    let app = router::build(inert_deps().app_state());
    let (status, body) = get_text(&app, "/mempool/counters").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    // the inert pool's DSN (host, user, pass) must never reach the client.
    assert!(
        !body.contains("127.0.0.1") && !body.contains("user") && !body.contains("pass"),
        "response body must not leak the underlying repository error: {body}"
    );
}
