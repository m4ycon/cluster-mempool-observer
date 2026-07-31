use api::infra::router;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use testkit::deps::inert_deps;
use testkit::metrics::{assert_no_series, assert_series, capture};
use tower::ServiceExt;

/// The real router, over inert dependencies.
fn app() -> Router {
    let state = inert_deps().app_state();
    router::build(state)
}

async fn call(app: &Router, path: &str) -> StatusCode {
    let request = Request::builder().uri(path).body(Body::empty()).unwrap();
    app.clone().oneshot(request).await.unwrap().status()
}

#[test]
fn plain_http_routes_get_a_latency_histogram() {
    let rendered = capture(async {
        let app = app();
        assert_eq!(call(&app, "/health").await, StatusCode::OK);
        assert_eq!(call(&app, "/mempool/txids").await, StatusCode::OK);
    });

    assert_series(
        &rendered,
        r#"axum_http_requests_total{method="GET",status="200",endpoint="/health"} 1"#,
    );
    assert_series(
        &rendered,
        r#"axum_http_requests_total{method="GET",status="200",endpoint="/mempool/txids"} 1"#,
    );
    assert!(
        rendered.contains("axum_http_requests_duration_seconds"),
        "no request histogram emitted:\n{rendered}"
    );
}

/// Websocket routes are excluded from the layer: their request duration is the
/// client's session length, which would swamp the buckets while saying nothing
/// about server latency.
///
/// The status assertions matter -- they prove the routes were reached and
/// ignored. Without them, an ignore pattern that stopped matching would look
/// identical to a route that was never called.
#[test]
fn websocket_routes_are_excluded() {
    let ws_routes = ["/mempool/delta", "/mempool/stats", "/clusters/delta"];

    let rendered = capture(async {
        let app = app();
        for path in ws_routes {
            assert_eq!(call(&app, path).await, StatusCode::BAD_REQUEST, "{path}");
        }
    });

    // Nothing was recorded at all: these were the only requests made.
    assert_no_series(&rendered, "axum_http_requests_total");
    for path in ws_routes {
        assert!(
            !rendered.contains(path),
            "expected {path} to be ignored:\n{rendered}"
        );
    }
}

#[test]
fn unmatched_paths_collapse_into_one_series() {
    let rendered = capture(async {
        let app = app();
        for path in ["/does-not-exist-abc123", "/nope-xyz789", "/admin.php"] {
            assert_eq!(call(&app, path).await, StatusCode::NOT_FOUND, "{path}");
        }
    });

    assert_series(
        &rendered,
        r#"axum_http_requests_total{method="GET",status="404",endpoint="unmatched"} 3"#,
    );
    for path in ["does-not-exist-abc123", "nope-xyz789", "admin.php"] {
        assert!(
            !rendered.contains(path),
            "raw URI leaked into a label:\n{rendered}"
        );
    }
}
