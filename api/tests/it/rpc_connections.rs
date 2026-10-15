#![cfg(all(feature = "db_integration_tests", feature = "node_integration_tests"))]

use api::db::Repos;
use api::db::schema::cluster_chunks;
use api::infra::deps::Deps;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use observer::clients::Clients;
use std::collections::HashSet;
use std::net::SocketAddr;
use testkit::config::get_config_with_rpc_config;
use testkit::fixtures::seed_txs;
use testkit::forwarder::Forwarder;
use testkit::metrics::{assert_no_series, local_recorder};
use testkit::node::{rpc_config_via, send_to_address, setup_node_with_args};
use testkit::postgres::isolated_pool;

/// The empty-DB bootstrap hands every live txid to the cluster sync, one
/// `getmempoolcluster` per singleton, back to back.
#[tokio::test]
async fn cluster_sync_of_a_fresh_mempool_shares_one_connection() {
    const TXS: usize = 12;

    // scenario: TXS mature coinbases and no spending of unconfirmed change, so
    // each send spends a coinbase of its own and stays a singleton
    let node = setup_node_with_args(&["-spendzeroconfchange=0"]);
    let address = node.client.new_address().expect("new address");
    node.client
        .generate_to_address(100 + TXS, &address)
        .expect("mature one coinbase per tx");
    let txids: Vec<String> = (0..TXS)
        .map(|_| send_to_address(&node, &address).to_string())
        .collect();

    let forwarder = Forwarder::start(SocketAddr::from(node.params.rpc_socket));
    let mut config = get_config_with_rpc_config(&node);
    config.rpc = rpc_config_via(&node, forwarder.addr());
    let pool = isolated_pool().await;
    let deps = Deps::new(
        Repos::new(pool.clone()),
        &Clients::new(&config).expect("init node clients"),
    );
    let seeded: Vec<&str> = txids.iter().map(String::as_str).collect();
    seed_txs(&deps.repos.transaction, &seeded).await;

    // execution
    let (recorder, handle) = local_recorder();
    let guard = metrics::set_default_local_recorder(&recorder);
    deps.cluster_service().sync_clusters_for(&txids, &[]).await;
    drop(guard);
    handle.run_upkeep();
    let rendered = handle.render();

    // assertion
    assert_eq!(forwarder.connections(), 1);
    assert_no_series(&rendered, "rpc_call_errors_total");
    let mut conn = pool.get().await.expect("conn");
    let clustered: HashSet<String> = cluster_chunks::table
        .filter(cluster_chunks::live.eq(true))
        .select(cluster_chunks::txids)
        .load::<Vec<String>>(&mut conn)
        .await
        .expect("load live chunks")
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(clustered, txids.into_iter().collect::<HashSet<_>>());
}
