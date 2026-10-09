#![cfg(feature = "db_integration_tests")]

use api::db::SystemEventRepository;
use api::db::models::SystemEventKind;
use api::services::node_health::NodeHealthService;
use api::services::system_event::SystemEventService;
use testkit::fixtures::BlockchainInfoFixture;
use testkit::mocks::MockNetworkRetriever;
use testkit::postgres::isolated_pool;

fn service(
    pool: api::db::DbPool,
    network: MockNetworkRetriever,
) -> NodeHealthService<MockNetworkRetriever> {
    let system_event_service = SystemEventService::new(SystemEventRepository::new(pool));
    NodeHealthService::new(system_event_service, network)
}

async fn kinds(pool: &api::db::DbPool) -> Vec<SystemEventKind> {
    SystemEventRepository::new(pool.clone())
        .list(None, None)
        .await
        .expect("list system events")
        .into_iter()
        .map(|row| row.kind)
        .collect()
}

#[tokio::test]
async fn repeated_identical_observations_write_exactly_one_row() {
    let pool = isolated_pool().await;
    let health = service(
        pool.clone(),
        MockNetworkRetriever::with_subversion("Satoshi:27.0.0"),
    );
    let info = BlockchainInfoFixture::new(800_000).build();

    health.observe_reachable(&info).await;
    health.observe_reachable(&info).await;
    health.observe_reachable(&info).await;

    let events = kinds(&pool).await;
    assert_eq!(
        events
            .iter()
            .filter(|k| **k == SystemEventKind::NodeConnected)
            .count(),
        1,
        "unchanged reachability must not flood the log"
    );
}

#[tokio::test]
async fn a_flap_writes_connected_disconnected_connected_in_order() {
    let pool = isolated_pool().await;
    let health = service(
        pool.clone(),
        MockNetworkRetriever::with_subversion("Satoshi:27.0.0"),
    );
    let info = BlockchainInfoFixture::new(800_000).build();

    health.observe_reachable(&info).await;
    health.observe_unreachable("connection refused").await;
    health.observe_reachable(&info).await;

    let events: Vec<SystemEventKind> = kinds(&pool)
        .await
        .into_iter()
        .filter(|k| {
            matches!(
                k,
                SystemEventKind::NodeConnected | SystemEventKind::NodeDisconnected
            )
        })
        .collect();

    assert_eq!(
        events,
        vec![
            SystemEventKind::NodeConnected,
            SystemEventKind::NodeDisconnected,
            SystemEventKind::NodeConnected,
        ]
    );
}

#[tokio::test]
async fn first_ever_connect_writes_version_changed_with_null_from() {
    let pool = isolated_pool().await;
    let health = service(
        pool.clone(),
        MockNetworkRetriever::with_subversion("Satoshi:27.0.0"),
    );
    let info = BlockchainInfoFixture::new(800_000).build();

    health.observe_reachable(&info).await;

    let repo = SystemEventRepository::new(pool);
    let row = repo
        .latest_of_kind(SystemEventKind::NodeVersionChanged)
        .await
        .expect("query latest node_version_changed")
        .expect("a node_version_changed row must exist");

    assert!(row.details["from"].is_null());
    assert_eq!(row.details["to"], "Satoshi:27.0.0");
}

#[tokio::test]
async fn reconnecting_with_unchanged_subversion_writes_no_new_version_row() {
    let pool = isolated_pool().await;
    let network = MockNetworkRetriever::with_subversion("Satoshi:27.0.0");
    let health = service(pool.clone(), network);
    let info = BlockchainInfoFixture::new(800_000).build();

    health.observe_reachable(&info).await;
    health.observe_unreachable("connection refused").await;
    health.observe_reachable(&info).await;

    let events = kinds(&pool).await;
    assert_eq!(
        events
            .iter()
            .filter(|k| **k == SystemEventKind::NodeVersionChanged)
            .count(),
        1,
        "same subversion on reconnect must not write a second version row"
    );
}

#[tokio::test]
async fn reconnecting_with_changed_subversion_writes_one_row_with_previous_from() {
    let pool = isolated_pool().await;
    let network = MockNetworkRetriever::with_subversion("Satoshi:27.0.0");
    let health = service(pool.clone(), network.clone());
    let info = BlockchainInfoFixture::new(800_000).build();

    health.observe_reachable(&info).await;
    health.observe_unreachable("connection refused").await;
    network.set_subversion("Satoshi:28.0.0");
    health.observe_reachable(&info).await;

    let repo = SystemEventRepository::new(pool);
    let rows: Vec<_> = repo
        .list(None, None)
        .await
        .expect("list system events")
        .into_iter()
        .filter(|row| row.kind == SystemEventKind::NodeVersionChanged)
        .collect();

    assert_eq!(
        rows.len(),
        2,
        "the subversion change must write a second row"
    );
    assert_eq!(rows[0].details["from"], serde_json::Value::Null);
    assert_eq!(rows[0].details["to"], "Satoshi:27.0.0");
    assert_eq!(rows[1].details["from"], "Satoshi:27.0.0");
    assert_eq!(rows[1].details["to"], "Satoshi:28.0.0");
}

#[tokio::test]
async fn getnetworkinfo_failing_still_connects_and_skips_the_version_check() {
    let pool = isolated_pool().await;
    let health = service(pool.clone(), MockNetworkRetriever::failing());
    let info = BlockchainInfoFixture::new(800_000).build();

    health.observe_reachable(&info).await;

    let repo = SystemEventRepository::new(pool);
    let connected = repo
        .list(None, None)
        .await
        .expect("list system events")
        .into_iter()
        .find(|row| row.kind == SystemEventKind::NodeConnected)
        .expect("node_connected must still be recorded");
    assert!(connected.details["subversion"].is_null());

    assert!(
        repo.latest_of_kind(SystemEventKind::NodeVersionChanged)
            .await
            .expect("query latest node_version_changed")
            .is_none(),
        "no version row when the version check could not run"
    );
}
