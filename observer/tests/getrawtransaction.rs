#![cfg(all(feature = "node_integration_tests", feature = "nats_integration_tests"))]

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
    let mut subscriber = server.subscribe(&Subject::RawTransaction).await;

    let node = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);

    // execution
    let config = get_config_with_rpc_config(&node);
    let runner = tokio::spawn(async move { run(&config).await });

    let event: GetRawTransactionEvent = call_retriever(RetrieverCall {
        subscriber: &mut subscriber,
        request_subject: &Subject::RequestRawTransaction,
        request_params: &txid.to_string(),
    })
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
