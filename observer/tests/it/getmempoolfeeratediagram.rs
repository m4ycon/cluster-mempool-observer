#![cfg(feature = "node_integration_tests")]

use observer::retrievers::{TransactionRetriever, TransactionRpcRetriever};
use observer::watchers::feerate_diagram::FeerateDiagramWatcher;
use observer::watchers::watcher_trait::WatcherRPC;
use shared::snapshot::FeerateDiagramSnapshot;
use testkit::node::{maturate_coinbase, send_to_address, setup_node_and_rpc_client};

#[tokio::test]
async fn getmempoolfeeratediagram_should_end_at_the_mempools_total_fee() {
    // scenario
    let (node, rpc) = setup_node_and_rpc_client();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address);
    let entry = TransactionRpcRetriever::new(rpc.clone())
        .get_mempool_entry(&txid.to_string())
        .await
        .expect("retrieve mempool entry");
    let snapshot = FeerateDiagramSnapshot::default();
    let mut watcher = FeerateDiagramWatcher::new(rpc, 1, snapshot.clone());

    // execution
    let changed = watcher.watch().await.expect("poll feerate diagram");

    // assertion
    assert!(changed.is_some(), "first poll should report a change");
    let points = snapshot.get().points;
    let last = points.last().expect("diagram should have points");
    assert!(last.weight > 0, "diagram should cover the tx's weight");
    assert_eq!(last.fee_sats, entry.fee_in_sats as i64);
}
