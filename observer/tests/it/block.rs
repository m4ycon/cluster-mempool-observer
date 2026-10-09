#![cfg(feature = "node_integration_tests")]

use futures::StreamExt;
use observer::clients::Clients;
use observer::runner::run;
use shared::events::BlockConnectedEvent;
use shared::snapshot::{FeerateDiagramSnapshot, MempoolLedger};
use shared::subjects::Subject;
use std::time::Duration;
use testkit::config::get_config_with_zmq_blocks;
use testkit::node::setup_node_with_zmq_hashblock;

#[tokio::test]
async fn block_watcher_should_publish_to_bus() {
    // scenario
    let (node, _rpc, zmq_endpoint) = setup_node_with_zmq_hashblock();
    let config = get_config_with_zmq_blocks(&node, zmq_endpoint);
    let clients = Clients::new(&config).expect("init node clients");

    // the bus has no replay, so subscribe before the watcher starts publishing
    let subscriber = clients.pubsub.subscribe(Subject::BlockConnected).await;
    futures::pin_mut!(subscriber);

    let runner = tokio::spawn(async move {
        run(
            &config,
            clients,
            MempoolLedger::default(),
            FeerateDiagramSnapshot::default(),
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    // execution: mine a block
    let address = node.client.new_address().expect("new address");
    let generated = node
        .client
        .generate_to_address(1, &address)
        .expect("mine 1 block");
    let mined_hash = generated.0.first().expect("one block hash").clone();

    // assertion
    let message = tokio::time::timeout(Duration::from_secs(5), subscriber.next())
        .await
        .expect("bus message within timeout")
        .expect("subscription yielded a message");

    let event: BlockConnectedEvent =
        serde_json::from_slice(&message.payload).expect("deserialize event payload");
    assert_eq!(
        event.hash, mined_hash,
        "published block hash should match the mined block"
    );

    runner.abort();
}
