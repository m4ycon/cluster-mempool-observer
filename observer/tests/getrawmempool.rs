// DISABLED reference: NATS removed in step 1 (placeholder pub-sub). Kept as a
// reference for restoring real-transport integration tests in step 4.
// `cfg(any())` is always false, so this file never compiles.
#![cfg(any())]

use futures::StreamExt;
use observer::clients::Clients;
use observer::runner::run;
use shared::events::GetRawMempoolEvent;
use shared::subjects::Subject;
use std::time::Duration;
use testkit::config::get_config_with_rpc_config;
use testkit::nats_server::NatsServerForTesting;
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};

#[tokio::test]
async fn getrawmempool_should_publish_mempool_delta_to_nats() {
    // scenario
    let server = NatsServerForTesting::start_nats().await;
    let nats = server.client().clone();
    let mut subscriber = server.subscribe(&Subject::RawMempool).await;

    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    send_to_address(&node, &node_address);

    // execution
    let config = get_config_with_rpc_config(&node);
    let clients = Clients { nats, rpc };
    let runner = tokio::spawn(async move { run(&config, clients).await });

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
