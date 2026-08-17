#![cfg(feature = "db_integration_tests")]

use api::db::SystemEventRepository;
use api::db::models::{SystemEventKind, SystemEventRow};
use shared::events::NodeStatusEvent;
use shared::subjects::Subject;
use std::time::Duration;
use testkit::deps::deps;
use testkit::fixtures::BlockchainInfoFixture;
use testkit::postgres::isolated_pool;

/// Polls until at least `expected_len` rows landed, since the consumer applies
/// events off the pubsub bus asynchronously.
async fn wait_for_events(repo: &SystemEventRepository, expected_len: usize) -> Vec<SystemEventRow> {
    for _ in 0..200 {
        let events = repo.list(None, None).await.expect("list system events");
        if events.len() >= expected_len {
            return events;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {expected_len} system events");
}

#[tokio::test]
async fn consumer_forwards_events_into_node_health_with_no_duplicate_transitions() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone());
    let repo = SystemEventRepository::new(pool);

    let node_status_service = deps.node_status_service();
    let stream = node_status_service.get_status_stream().await;
    tokio::spawn(async move { node_status_service.consume(stream).await });

    let reachable = NodeStatusEvent::reachable(&BlockchainInfoFixture::new(100).build());
    let unreachable = NodeStatusEvent::unreachable("connection refused");

    // reachable, reachable (dup), unreachable, unreachable (dup), reachable (recovery)
    for event in [
        &reachable,
        &reachable,
        &unreachable,
        &unreachable,
        &reachable,
    ] {
        deps.pubsub.publish(Subject::NodeStatus, event).await;
    }

    let events = wait_for_events(&repo, 3).await;

    assert_eq!(
        events.len(),
        3,
        "repeated observations of the same state must not add rows"
    );
    assert_eq!(events[0].kind, SystemEventKind::NodeConnected);
    assert_eq!(events[1].kind, SystemEventKind::NodeDisconnected);
    assert_eq!(events[2].kind, SystemEventKind::NodeConnected);
}

#[tokio::test]
async fn first_observation_being_unreachable_is_still_recorded() {
    let pool = isolated_pool().await;
    let deps = deps(pool.clone());
    let repo = SystemEventRepository::new(pool);

    let node_status_service = deps.node_status_service();
    let stream = node_status_service.get_status_stream().await;
    tokio::spawn(async move { node_status_service.consume(stream).await });

    deps.pubsub
        .publish(
            Subject::NodeStatus,
            &NodeStatusEvent::unreachable("timed out"),
        )
        .await;

    let events = wait_for_events(&repo, 1).await;

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, SystemEventKind::NodeDisconnected);
}
