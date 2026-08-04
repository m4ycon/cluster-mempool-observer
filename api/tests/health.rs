use api::infra::readiness::Phase;
use api::infra::router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use testkit::deps::inert_deps;
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
async fn health_reports_the_wait_and_data_routes_are_gated() {
    let deps = inert_deps();
    deps.readiness.set_phase(Phase::WaitingForNode);
    let app = router::build(deps.app_state());

    // Health answers regardless -- the process is up, that is what it reports.
    let (status, body) = get(&app, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ready"], false);
    assert_eq!(body["phase"], "waiting_for_node");
    assert_eq!(body["node"]["reachable"], false);

    // Data routes must not answer with empty state, which reads as "no mempool".
    let (status, body) = get(&app, "/mempool/txids").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["phase"], "waiting_for_node");
}

#[tokio::test]
async fn bootstrapping_is_reported_and_still_gated() {
    let deps = inert_deps();
    deps.readiness.set_phase(Phase::Bootstrapping);
    let app = router::build(deps.app_state());

    let (_, body) = get(&app, "/health").await;
    assert_eq!(body["phase"], "bootstrapping");
    assert_eq!(body["ready"], false);

    let (status, _) = get(&app, "/mempool/txids").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn data_routes_open_once_ready() {
    // inert_deps is already marked ready: it stands in for a bootstrapped api.
    let app = router::build(inert_deps().app_state());

    let (status, body) = get(&app, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ready"], true);
    assert_eq!(body["phase"], "ready");

    let (status, _) = get(&app, "/mempool/txids").await;
    assert_eq!(status, StatusCode::OK);
}
