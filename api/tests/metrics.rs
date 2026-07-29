use api::db::build_pool;
use api::infra::router;
use api::infra::state::AppState;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use observer::infra::config::{Config as ObserverConfig, RpcConfig};
use tower::ServiceExt;

fn app() -> axum::Router {
    let config = ObserverConfig {
        rpc: RpcConfig {
            host: "127.0.0.1:18443".into(),
            user: "user".into(),
            pass: "pass".into(),
        },
        ..Default::default()
    };
    let pool = build_pool("postgres://user:pass@127.0.0.1:5432/unused").expect("build pool");
    let (state, _snapshot, _clients) = AppState::build(&config, pool);
    router::build(state)
}

/// Plain HTTP routes get latency histograms; the three websocket routes must
/// not, because their request duration is the client's session length and
/// would swamp the buckets.
#[tokio::test]
async fn records_http_routes_and_ignores_websocket_routes() {
    let handle = shared::metrics::init_metrics().expect("failed to install recorder");

    let app = app();
    let mut statuses = Vec::new();
    for path in [
        "/health",
        "/mempool/txids",
        "/mempool/delta",
        "/mempool/stats",
        "/clusters/delta",
    ] {
        let request = Request::builder().uri(path).body(Body::empty()).unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        statuses.push((path, response.status()));
    }

    // The websocket routes are reached -- they just refuse a non-upgrade GET.
    // Without this, an ignore pattern that silently stopped matching would look
    // identical to a route that was never called.
    assert_eq!(statuses[0].1, StatusCode::OK, "/health");
    assert_eq!(statuses[1].1, StatusCode::OK, "/mempool/txids");
    for (path, status) in &statuses[2..] {
        assert_eq!(*status, StatusCode::BAD_REQUEST, "{path}");
    }

    handle.run_upkeep();
    let rendered = handle.render();

    assert!(
        rendered.contains("axum_http_requests_duration_seconds"),
        "no request histogram emitted:\n{rendered}"
    );
    for recorded in ["/health", "/mempool/txids"] {
        assert!(
            rendered.contains(recorded),
            "expected {recorded} to be recorded:\n{rendered}"
        );
    }
    for ignored in ["/mempool/delta", "/mempool/stats", "/clusters/delta"] {
        assert!(
            !rendered.contains(ignored),
            "expected {ignored} to be ignored:\n{rendered}"
        );
    }
}
