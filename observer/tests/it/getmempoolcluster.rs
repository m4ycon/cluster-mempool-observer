#![cfg(feature = "node_integration_tests")]

use observer::error::ObserverError;
use observer::retrievers::{
    ClusterRetriever, ClusterRpcRetriever, TransactionRetriever, TransactionRpcRetriever,
};
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};

#[tokio::test]
async fn getmempoolcluster_should_answer_with_the_txs_singleton_cluster() {
    // scenario
    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);
    let entry = TransactionRpcRetriever::new(rpc.clone())
        .get_mempool_entry(&txid.to_string())
        .await
        .expect("retrieve mempool entry");

    // execution
    let cluster = ClusterRpcRetriever::new(rpc)
        .get_mempool_cluster(&txid.to_string())
        .await
        .expect("retrieve mempool cluster");

    // assertion
    assert_eq!(cluster.tx_count, 1);
    assert_eq!(
        cluster.txids().cloned().collect::<Vec<_>>(),
        vec![txid.to_string()]
    );
    assert!(
        cluster.cluster_weight > 0,
        "cluster should carry its weight"
    );
    assert_eq!(
        cluster.total_fee_sats(),
        entry.fee_in_sats,
        "without prioritisetransaction the chunk fee is the base fee"
    );
}

#[tokio::test]
async fn getmempoolcluster_should_report_a_mined_tx_as_missing_from_the_mempool() {
    // scenario
    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);
    node.client
        .generate_to_address(1, &node_address)
        .expect("mine the tx");

    // execution
    let err = ClusterRpcRetriever::new(rpc)
        .get_mempool_cluster(&txid.to_string())
        .await
        .expect_err("a mined tx has no cluster");

    // assertion
    assert!(
        matches!(err, ObserverError::TxNotFoundInMempool(_)),
        "expected TxNotFoundInMempool, got {err}"
    );
}
