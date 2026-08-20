#![cfg(feature = "db_integration_tests")]

use api::db::TransactionRepository;
use api::infra::readiness::Phase;
use api::infra::router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use testkit::deps::{deps, inert_deps};
use testkit::fixtures::{TxFixture, hex_txid};
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
async fn found_comes_back_in_request_order_and_missing_lists_the_rest() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool.clone());
    let a = hex_txid("a");
    let b = hex_txid("b");
    let c = hex_txid("c"); // never seeded

    repo.insert(&TxFixture::new(&a).build())
        .await
        .expect("seed a");
    repo.insert(&TxFixture::new(&b).build())
        .await
        .expect("seed b");

    let app = router::build(deps(pool).app_state());

    // Request order deliberately differs from insertion order: c, b, a.
    let (status, body) = get(&app, &format!("/transactions?txids={c},{b},{a}")).await;

    assert_eq!(status, StatusCode::OK);
    let found = body["found"].as_array().expect("found array");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0]["txid"], b.as_str());
    assert_eq!(found[1]["txid"], a.as_str());
    assert_eq!(body["missing"], serde_json::json!([c]));
}

#[tokio::test]
async fn input_txids_none_and_empty_survive_through_the_router_distinctly() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool.clone());
    let unknown = hex_txid("1");
    let coinbase = hex_txid("2");

    repo.insert(&TxFixture::new(&unknown).sized().build())
        .await
        .expect("seed unknown-parents tx");
    repo.insert(
        &TxFixture::new(&coinbase)
            .sized()
            .with_input_txids(&[])
            .build(),
    )
    .await
    .expect("seed coinbase tx");

    let app = router::build(deps(pool).app_state());

    let (status, body) = get(&app, &format!("/transactions?txids={unknown},{coinbase}")).await;

    assert_eq!(status, StatusCode::OK);
    let found = body["found"].as_array().expect("found array");
    let unknown_ref = found
        .iter()
        .find(|r| r["txid"] == unknown.as_str())
        .unwrap();
    let coinbase_ref = found
        .iter()
        .find(|r| r["txid"] == coinbase.as_str())
        .unwrap();
    assert_eq!(unknown_ref["input_txids"], Value::Null);
    assert_eq!(coinbase_ref["input_txids"], serde_json::json!([]));
}

#[tokio::test]
async fn a_hollow_row_serializes_null_fee_zero_vsize_and_hollow_true() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool.clone());
    let txid = hex_txid("3");

    repo.insert(&TxFixture::new(&txid).build())
        .await
        .expect("seed hollow tx");

    let app = router::build(deps(pool).app_state());
    let (status, body) = get(&app, &format!("/transactions?txids={txid}")).await;

    assert_eq!(status, StatusCode::OK);
    let row = &body["found"][0];
    assert_eq!(row["fee"], Value::Null);
    assert_eq!(row["vsize"], 0);
    assert_eq!(row["hollow"], true);
}

#[tokio::test]
async fn duplicate_txids_in_the_query_collapse_to_one_entry() {
    let pool = isolated_pool().await;
    let repo = TransactionRepository::new(pool.clone());
    let txid = hex_txid("4");

    repo.insert(&TxFixture::new(&txid).build())
        .await
        .expect("seed tx");

    let app = router::build(deps(pool).app_state());
    let (status, body) = get(&app, &format!("/transactions?txids={txid},{txid}")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["found"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn too_many_txids_is_rejected() {
    let app = router::build(inert_deps().app_state());
    let raw = vec![hex_txid("a"); 129].join(",");

    let (status, body) = get_text(&app, &format!("/transactions?txids={raw}")).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.is_empty());
}

#[tokio::test]
async fn an_invalid_txid_is_rejected() {
    let app = router::build(inert_deps().app_state());

    let (status, _) = get(&app, "/transactions?txids=not-a-txid").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_unready_app_gates_the_route() {
    let deps = inert_deps();
    deps.readiness.set_phase(Phase::WaitingForNode);
    let app = router::build(deps.app_state());

    let (status, _) = get(&app, &format!("/transactions?txids={}", hex_txid("a"))).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
