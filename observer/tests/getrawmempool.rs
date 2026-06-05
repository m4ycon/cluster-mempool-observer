#![cfg(feature = "node_integration_tests")]

use common::setup_node_and_rpc_client;
use corepc_node::serde_json::{Value, json};
use observer::extractors::extractor_trait::Extractor;
use observer::extractors::getrawmempool::GetRawMempoolExtractor;

mod common;

#[tokio::test]
async fn getrawmempool_should_return_mempool_txids_from_a_real_node() {
    let node = setup_node_and_rpc_client();

    // mature a coinbase, then broadcast one tx that stays unconfirmed
    let address = node.client.new_address().expect("new address");
    node.client
        .generate_to_address(101, &address)
        .expect("mine 101 blocks");
    node.client
        .call::<Value>("sendtoaddress", &[json!(address.to_string()), json!(1.0)])
        .expect("send to address");

    let mut extractor = GetRawMempoolExtractor;
    let response = extractor.extract().await.expect("extract mempool");
    let event = extractor.into_event(&response);

    assert_eq!(
        event.txids.len(),
        1,
        "exactly one unconfirmed tx should be in the mempool"
    );
    assert!(extractor.can_extract_again());
}
