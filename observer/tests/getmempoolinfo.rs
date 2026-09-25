#![cfg(feature = "node_integration_tests")]

use observer::retrievers::{MempoolRetriever, MempoolRpcRetriever};
use std::time::Duration;
use testkit::node::setup_node_and_rpc_client;

#[tokio::test]
async fn getmempoolinfo_should_report_the_mempool_loaded() {
    // scenario
    let (_node, rpc) = setup_node_and_rpc_client();
    let retriever = MempoolRpcRetriever::new(rpc);

    // execution: RPC comes up before the node's load thread finishes, so a
    // fresh node may briefly answer `loaded: false`
    let mut loaded = false;
    for _ in 0..50 {
        let info = retriever
            .get_mempool_info()
            .await
            .expect("retrieve mempool info");
        if info.loaded {
            loaded = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // assertion
    assert!(loaded, "a regtest node has an empty mempool.dat to load");
}
