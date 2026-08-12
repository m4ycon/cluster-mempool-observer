#![cfg(feature = "node_integration_tests")]

use futures::StreamExt;
use observer::clients::Clients;
use observer::runner::run;
use shared::events::MempoolDeltaEvent;
use shared::snapshot::{FeerateDiagramSnapshot, MempoolSnapshot};
use shared::subjects::Subject;
use std::time::Duration;
use testkit::config::get_config_with_rpc_config;
use testkit::node::{maturate_coinbase, send_to_address, setup_node};

#[tokio::test]
async fn mempool_delta_should_publish_to_bus() {
    // scenario
    let node = setup_node();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    send_to_address(&node, &node_address);

    let config = get_config_with_rpc_config(&node);
    let clients = Clients::new(&config).expect("init node clients");

    // the bus has no replay, so subscribe before the watcher starts publishing
    let subscriber = clients.pubsub.subscribe(Subject::MempoolDelta).await;
    futures::pin_mut!(subscriber);

    // execution
    let runner = tokio::spawn(async move {
        run(
            &config,
            clients,
            MempoolSnapshot::default(),
            FeerateDiagramSnapshot::default(),
        )
        .await
    });

    let message = tokio::time::timeout(Duration::from_secs(5), subscriber.next())
        .await
        .expect("bus message within timeout")
        .expect("subscription yielded a message");

    // assertion
    let event: MempoolDeltaEvent =
        serde_json::from_slice(&message.payload).expect("deserialize event payload");
    assert_eq!(
        event.added.len(),
        1,
        "exactly one unconfirmed tx should be reported as added"
    );
    assert!(event.removed.is_empty());

    runner.abort();
}
