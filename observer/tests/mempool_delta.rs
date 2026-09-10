#![cfg(feature = "node_integration_tests")]

use observer::clients::Clients;
use observer::runner::run;
use shared::models::DeltaDirection;
use shared::snapshot::{FeerateDiagramSnapshot, MempoolLedger};
use std::time::Duration;
use testkit::config::get_config_with_rpc_config;
use testkit::node::{maturate_coinbase, send_to_address, setup_node};
use testkit::wait::wait_for;

#[tokio::test]
async fn mempool_delta_watcher_submits_the_polled_txid_to_the_ledger() {
    // scenario
    let node = setup_node();
    let node_address = node.client.new_address().expect("new address");
    maturate_coinbase(&node, &node_address);
    let txid = send_to_address(&node, &node_address).to_string();

    let config = get_config_with_rpc_config(&node);
    let clients = Clients::new(&config).expect("init node clients");
    let ledger = MempoolLedger::default();

    // execution: the watcher no longer publishes an event (the reconciler owns
    // that, on the net delta of a whole flush) -- it feeds the ledger instead,
    // so that's what this proves against.
    let runner = tokio::spawn({
        let ledger = ledger.clone();
        async move { run(&config, clients, ledger, FeerateDiagramSnapshot::default()).await }
    });

    wait_for(Duration::from_secs(5), || async {
        ledger.contains(&txid).then_some(())
    })
    .await
    .expect("ledger should hold the sent tx after a poll");

    // assertion: not just resident, but still an unflushed Add transition --
    // nothing in this test drains the journal.
    let batch = ledger
        .begin_flush()
        .expect("the add must still be pending in the journal");
    assert!(
        batch
            .entries()
            .iter()
            .any(|e| e.txid == txid && e.direction == DeltaDirection::Add),
        "the txid must appear as a pending Add transition in the journal"
    );

    runner.abort();
}
