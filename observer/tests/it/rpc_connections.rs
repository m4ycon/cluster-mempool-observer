#![cfg(feature = "node_integration_tests")]

use futures::future::join_all;
use observer::clients::rpc_client::RpcClient;
use observer::error::ObserverError;
use observer::retrievers::{BlockRetriever, BlockRpcRetriever};
use std::net::SocketAddr;
use std::time::Duration;
use testkit::forwarder::Forwarder;
use testkit::metrics::{assert_series, local_recorder};
use testkit::node::{BitcoinD, rpc_config_via, setup_node, setup_node_with_args};

/// Short enough that a test can wait out the node dropping an idle connection.
const RPC_SERVER_TIMEOUT: &str = "-rpcservertimeout=1";

fn tip_retriever_via(node: &BitcoinD, forwarder: &Forwarder) -> BlockRpcRetriever {
    let rpc = RpcClient::new(&rpc_config_via(node, forwarder.addr())).expect("init rpc client");
    BlockRpcRetriever::new(rpc)
}

#[tokio::test]
async fn sequential_rpc_calls_share_one_connection() {
    // scenario
    let node = setup_node();
    let forwarder = Forwarder::start(SocketAddr::from(node.params.rpc_socket));
    let retriever = tip_retriever_via(&node, &forwarder);

    // execution
    for _ in 0..50 {
        retriever.get_tip_height().await.expect("tip height");
    }

    // assertion
    assert_eq!(forwarder.connections(), 1);
}

#[tokio::test]
async fn concurrent_rpc_calls_open_at_most_one_connection_each() {
    const CONCURRENT: usize = 8;

    // scenario
    let node = setup_node();
    let forwarder = Forwarder::start(SocketAddr::from(node.params.rpc_socket));
    let retriever = tip_retriever_via(&node, &forwarder);

    // execution
    for _ in 0..5 {
        let round = join_all((0..CONCURRENT).map(|_| retriever.get_tip_height())).await;
        for tip in round {
            tip.expect("tip height");
        }
    }

    // assertion
    let opened = forwarder.connections();
    assert!(
        (1..=CONCURRENT).contains(&opened),
        "expected 1..={CONCURRENT} connections, got {opened}"
    );
}

#[tokio::test]
async fn a_connection_the_node_dropped_while_idle_is_replaced() {
    // scenario
    let node = setup_node_with_args(&[RPC_SERVER_TIMEOUT]);
    let forwarder = Forwarder::start(SocketAddr::from(node.params.rpc_socket));
    let retriever = tip_retriever_via(&node, &forwarder);
    retriever.get_tip_height().await.expect("tip height");

    // execution
    tokio::time::sleep(Duration::from_secs(2)).await;
    let after_idle = retriever.get_tip_height().await;

    // assertion
    after_idle.expect("tip height after the node dropped the idle connection");
    assert_eq!(forwarder.connections(), 2);
}

#[tokio::test]
async fn a_refused_reconnect_fails_the_call_until_connections_are_accepted_again() {
    const COOLDOWN: Duration = Duration::from_secs(4);

    // scenario: one connection, dropped by the node while idle, and no other
    // accepted until the cooldown ends
    let node = setup_node_with_args(&[RPC_SERVER_TIMEOUT]);
    let forwarder = Forwarder::with_budget(SocketAddr::from(node.params.rpc_socket), 1, COOLDOWN);
    let retriever = tip_retriever_via(&node, &forwarder);
    retriever
        .get_tip_height()
        .await
        .expect("tip height within budget");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // execution
    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    let refused = retriever.get_tip_height().await;
    drop(guard);
    tokio::time::sleep(COOLDOWN).await;
    let accepted = retriever.get_tip_height().await;

    // assertion
    match refused {
        Err(ObserverError::FailedToFetch(e)) => assert!(
            e.contains("Connection refused (os error 111)"),
            "expected a refused connect, got: {e}"
        ),
        other => panic!("expected FailedToFetch, got {other:?}"),
    }
    handle.run_upkeep();
    assert_series(
        &handle.render(),
        r#"rpc_call_errors_total{method="getblockcount"} 1"#,
    );
    accepted.expect("tip height once connections are accepted again");
    assert_eq!(forwarder.connections(), 2);
}
