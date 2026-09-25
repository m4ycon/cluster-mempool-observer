#![cfg(feature = "node_integration_tests")]

use observer::retrievers::{MempoolRetriever, MempoolRpcRetriever};
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};

#[tokio::test]
async fn getrawmempoolverbose_should_answer_request_with_mempool_entries() {
    // scenario
    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);

    // execution
    let retriever = MempoolRpcRetriever::new(rpc);
    let event = retriever
        .get_raw_mempool_verbose()
        .await
        .expect("retrieve verbose mempool");

    // assertion
    assert!(
        !event.entries.is_empty(),
        "verbose mempool should carry at least one entry"
    );
    assert!(
        event.entries.iter().any(|e| e.txid == txid.to_string()),
        "verbose mempool should contain the sent txid"
    );
}
