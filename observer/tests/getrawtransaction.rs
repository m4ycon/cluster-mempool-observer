// DISABLED reference: NATS removed in step 1 (placeholder pub-sub). Kept as a
// reference for restoring real-transport integration tests in step 4.
// `cfg(any())` is always false, so this file never compiles.
#![cfg(any())]

use observer::clients::Clients;
use observer::runner::run;
use shared::events::GetRawTransactionEvent;
use shared::subjects::Subject;
use testkit::call_retriever::{RetrieverCall, call_retriever};
use testkit::config::get_config_with_rpc_config;
use testkit::nats_server::NatsServerForTesting;
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};

#[tokio::test]
async fn getrawtransaction_should_answer_request_with_raw_transaction() {
    // scenario
    let server = NatsServerForTesting::start_nats().await;
    let nats = server.client().clone();
    let mut subscriber = server.subscribe(&Subject::RawTransaction).await;

    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);

    // execution
    let config = get_config_with_rpc_config(&node);
    let clients = Clients {
        nats: nats.clone(),
        rpc,
    };
    let runner = tokio::spawn(async move { run(&config, clients).await });

    let event: GetRawTransactionEvent = call_retriever(
        &nats,
        RetrieverCall {
            subscriber: &mut subscriber,
            request_subject: &Subject::RequestRawTransaction,
            request_params: &txid.to_string(),
        },
    )
    .await;

    // assertion
    assert_eq!(
        event.txid,
        txid.to_string(),
        "answer should carry the requested txid"
    );
    assert!(
        !event.hex.is_empty(),
        "event should carry the serialized tx"
    );

    runner.abort();
}
