#![cfg(feature = "node_integration_tests")]

use observer::clients::Clients;
use observer::runner::run;
use shared::events::GetRawMempoolVerboseEvent;
use shared::pubsub::PubSub;
use shared::subjects::Subject;
use testkit::call_retriever::{RetrieverCall, call_retriever};
use testkit::config::get_config_with_rpc_config;
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};

#[tokio::test]
async fn getrawmempoolverbose_should_answer_request_with_mempool_entries() {
    // scenario
    let pubsub = PubSub::new();
    let responses = pubsub.subscribe(Subject::RawMempoolVerbose).await;

    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);

    // execution
    let config = get_config_with_rpc_config(&node);
    let clients = Clients {
        pubsub: pubsub.clone(),
        rpc,
    };
    let runner = tokio::spawn(async move { run(&config, clients).await });

    let event: GetRawMempoolVerboseEvent = call_retriever(
        &pubsub,
        RetrieverCall {
            subscriber: responses,
            request_subject: Subject::RequestRawMempoolVerbose,
            request_params: &(),
        },
    )
    .await;

    // assertion
    assert!(
        !event.entries.is_empty(),
        "verbose mempool should carry at least one entry"
    );
    assert!(
        event.entries.iter().any(|e| e.txid == txid.to_string()),
        "verbose mempool should contain the sent txid"
    );

    runner.abort();
}
