#![cfg(feature = "node_integration_tests")]

use observer::error::ObserverError;
use observer::retrievers::{TransactionRetriever, TransactionRpcRetriever};
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};

#[tokio::test]
async fn getmempoolentry_should_answer_with_the_base_fee() {
    // scenario
    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);

    // execution
    let retriever = TransactionRpcRetriever::new(rpc);
    let entry = retriever
        .get_mempool_entry(&txid.to_string())
        .await
        .expect("retrieve mempool entry");

    // assertion
    assert_eq!(entry.txid, txid.to_string());
    assert!(entry.fee_in_sats > 0, "a wallet send pays a fee");
    assert!(entry.vsize > 0, "entry should carry the tx vsize");
}

#[tokio::test]
async fn getmempoolentry_should_report_a_mined_tx_as_missing_from_the_mempool() {
    // scenario
    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);
    node.client
        .generate_to_address(1, &node_address)
        .expect("mine the tx");

    // execution
    let retriever = TransactionRpcRetriever::new(rpc);
    let err = retriever
        .get_mempool_entry(&txid.to_string())
        .await
        .err()
        .expect("a mined tx has no mempool entry");

    // assertion
    assert!(
        matches!(err, ObserverError::TxNotFoundInMempool(_)),
        "expected TxNotFoundInMempool, got {err}"
    );
}
