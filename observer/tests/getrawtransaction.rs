#![cfg(feature = "node_integration_tests")]

use observer::retrievers::{TransactionRetriever, TransactionRpcRetriever};
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};

#[tokio::test]
async fn getrawtransaction_should_answer_request_with_raw_transaction() {
    // scenario
    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);

    // execution
    let retriever = TransactionRpcRetriever::new(rpc);
    let event = retriever
        .get_raw_transaction(&txid.to_string())
        .await
        .expect("retrieve raw transaction");

    // assertion
    assert_eq!(
        event.txid,
        txid.to_string(),
        "answer should carry the requested txid"
    );
    assert!(event.vsize > 0, "event should carry the tx vsize");
    assert!(
        event.output_count > 0,
        "event should carry the tx output count"
    );
}
