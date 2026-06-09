#![cfg(all(feature = "node_integration_tests", feature = "nats_integration_tests"))]

use futures::StreamExt;
use observer::extractors::getrawmempool::GetRawMempoolEvent;
use observer::infra::nats::Subject;
use observer::runner::run;
use shared::testing::config::get_config_with_rpc_config;
use shared::testing::nats_server::NatsServerForTesting;
use shared::testing::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};
use std::time::Duration;

#[tokio::test]
async fn getrawmempool_should_publish_mempool_delta_to_nats() {
    // scenario
    let server = NatsServerForTesting::start_nats().await;
    let mut subscriber = server.subscribe(&Subject::RawMempool).await;

    let node = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    send_to_address(&node, &node_address);

    // execution
    let config = get_config_with_rpc_config(&node);
    let runner = tokio::spawn(run(config));

    let message = tokio::time::timeout(Duration::from_secs(5), subscriber.next())
        .await
        .expect("nats message within timeout")
        .expect("subscription yielded a message");

    // assertion
    let event: GetRawMempoolEvent =
        serde_json::from_slice(&message.payload).expect("deserialize event payload");
    assert_eq!(
        event.added.len(),
        1,
        "exactly one unconfirmed tx should be reported as added"
    );
    assert!(event.removed.is_empty());

    runner.abort();
}
