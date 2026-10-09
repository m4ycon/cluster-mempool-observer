use api::infra::router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use shared::api::{FeerateDiagramPoint, MempoolFeerateDiagram};
use testkit::deps::inert_deps;
use testkit::fixtures::{FeerateDiagramFixture, TX_FEE, TX_VSIZE, WU_PER_VBYTE, fixed_time};
use time::format_description::well_known::Rfc3339;
use tower::ServiceExt;

const PATH: &str = "/mempool/feerate-diagram";

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
async fn cold_start_returns_null_sampled_at_and_no_points() {
    // A fresh snapshot was never written -- the route must still answer 200,
    // not 503, so a client can tell "no data yet" from "server not ready".
    let app = router::build(inert_deps().app_state());

    let (status, body) = get(&app, PATH).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!({ "sampled_at": null, "points": [] })
    );
}

#[tokio::test]
async fn populated_snapshot_is_served_with_its_points_and_sampled_at() {
    let deps = inert_deps();
    deps.feerate_diagram_snapshot
        .store(FeerateDiagramFixture::new().build());
    let app = router::build(deps.app_state());

    let (status, body) = get(&app, PATH).await;

    assert_eq!(status, StatusCode::OK);
    let expected_sampled_at = fixed_time().format(&Rfc3339).unwrap();
    assert_eq!(body["sampled_at"], expected_sampled_at.as_str());
    assert_eq!(
        body["points"],
        serde_json::json!([
            { "weight": 0, "fee_sats": 0 },
            { "weight": WU_PER_VBYTE * TX_VSIZE as u64, "fee_sats": TX_FEE },
            { "weight": 2 * WU_PER_VBYTE * TX_VSIZE as u64, "fee_sats": 2 * TX_FEE },
        ])
    );
}

#[tokio::test]
async fn points_keep_their_stored_order_and_the_origin_point_is_not_dropped() {
    let deps = inert_deps();
    // Distinct weight/fee_sats per point, so a reordering bug cannot hide
    // behind duplicate values.
    let points = vec![
        FeerateDiagramPoint {
            weight: 0,
            fee_sats: 0,
        },
        FeerateDiagramPoint {
            weight: 200,
            fee_sats: 90,
        },
        FeerateDiagramPoint {
            weight: 550,
            fee_sats: 310,
        },
        FeerateDiagramPoint {
            weight: 1_200,
            fee_sats: 900,
        },
    ];
    deps.feerate_diagram_snapshot.store(
        FeerateDiagramFixture::new()
            .with_points(points.clone())
            .build(),
    );
    let app = router::build(deps.app_state());

    let (status, body) = get(&app, PATH).await;
    assert_eq!(status, StatusCode::OK);
    let diagram: MempoolFeerateDiagram =
        serde_json::from_value(body).expect("response deserializes as MempoolFeerateDiagram");

    assert_eq!(diagram.points.len(), points.len());
    for (got, want) in diagram.points.iter().zip(points.iter()) {
        assert_eq!(got.weight, want.weight);
        assert_eq!(got.fee_sats, want.fee_sats);
    }
}
