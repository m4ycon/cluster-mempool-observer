#![cfg(feature = "db_integration_tests")]

use api::db::SystemEventRepository;
use api::db::models::SystemEventKind;
use api::infra::config::ApiConfig;
use api::infra::lifecycle::{self, ShutdownSignal};
use api::infra::router;
use api::services::system_event::SystemEventService;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use std::time::{Duration, Instant};
use testkit::deps::deps;
use testkit::fixtures::NewSystemEventFixture;
use testkit::postgres::isolated_pool;
use tower::ServiceExt;

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

#[tokio::test]
async fn list_returns_rows_oldest_first() {
    let pool = isolated_pool().await;
    let repo = SystemEventRepository::new(pool);

    for kind in [
        SystemEventKind::ServerStarted,
        SystemEventKind::NodeConnected,
        SystemEventKind::NodeDisconnected,
    ] {
        repo.insert(&NewSystemEventFixture::new(kind).build())
            .await
            .expect("insert system event");
    }

    let rows = repo.list(None, None).await.expect("list system events");

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].kind, SystemEventKind::ServerStarted);
    assert_eq!(rows[1].kind, SystemEventKind::NodeConnected);
    assert_eq!(rows[2].kind, SystemEventKind::NodeDisconnected);
    assert!(
        rows[0].id < rows[1].id && rows[1].id < rows[2].id,
        "ids must ascend oldest first"
    );
}

#[tokio::test]
async fn list_bounds_results_by_the_created_at_range() {
    let pool = isolated_pool().await;
    let repo = SystemEventRepository::new(pool);

    for kind in &SystemEventKind::ALL[..3] {
        repo.insert(&NewSystemEventFixture::new(*kind).build())
            .await
            .expect("insert system event");
    }

    // Oldest first, so [0] is the first row inserted and [2] the last.
    let full = repo.list(None, None).await.expect("list all");
    let middle = full[1].created_at;

    let since_middle = repo
        .list(Some(middle), None)
        .await
        .expect("list from middle");
    assert!(
        since_middle.iter().all(|row| row.created_at >= middle),
        "from must exclude anything older than the bound"
    );
    assert!(since_middle.iter().any(|row| row.id == full[1].id));
    assert!(since_middle.iter().any(|row| row.id == full[2].id));

    let until_middle = repo.list(None, Some(middle)).await.expect("list to middle");
    assert!(
        until_middle.iter().all(|row| row.created_at <= middle),
        "to must exclude anything newer than the bound"
    );
    assert!(until_middle.iter().any(|row| row.id == full[0].id));
    assert!(until_middle.iter().any(|row| row.id == full[1].id));
}

#[tokio::test]
async fn route_lists_system_events_oldest_first() {
    let pool = isolated_pool().await;
    let repo = SystemEventRepository::new(pool.clone());

    for kind in [
        SystemEventKind::BootstrapStarted,
        SystemEventKind::BootstrapCompleted,
    ] {
        repo.insert(&NewSystemEventFixture::new(kind).build())
            .await
            .expect("insert system event");
    }

    let app = router::build(deps(pool).app_state());
    let (status, body) = get(&app, "/system-events").await;

    assert_eq!(status, StatusCode::OK);
    let events = body.as_array().expect("body is a json array");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["kind"], "bootstrap_started");
    assert_eq!(events[1]["kind"], "bootstrap_completed");
}

#[tokio::test]
async fn startup_and_shutdown_are_recorded_in_order() {
    let pool = isolated_pool().await;
    let service = SystemEventService::new(SystemEventRepository::new(pool.clone()));
    let cfg = ApiConfig {
        bind: "0.0.0.0:4242".to_string(),
        ..Default::default()
    };

    let start = Instant::now();
    lifecycle::record_server_started(&service, &cfg).await;
    lifecycle::record_server_stopped(&service, start, async { ShutdownSignal::Term }).await;

    let events = service.list(None, None).await.expect("list system events");

    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].kind,
        shared::events::SystemEventKind::ServerStarted
    );
    assert_eq!(events[0].details["bind"], "0.0.0.0:4242");
    assert!(events[0].details["git_sha"].is_string());

    assert_eq!(
        events[1].kind,
        shared::events::SystemEventKind::ServerStopped
    );
    assert_eq!(events[1].details["signal"], "SIGTERM");
    assert!(events[1].details["uptime_secs"].is_number());
}

#[tokio::test]
async fn shutdown_is_not_recorded_until_the_signal_arrives() {
    let pool = isolated_pool().await;
    let service = SystemEventService::new(SystemEventRepository::new(pool.clone()));

    let pending =
        lifecycle::record_server_stopped(&service, Instant::now(), std::future::pending());
    let timed_out = tokio::time::timeout(Duration::from_millis(50), pending).await;

    assert!(timed_out.is_err(), "must still be waiting on the signal");
    assert!(
        service
            .list(None, None)
            .await
            .expect("list system events")
            .is_empty()
    );
}
