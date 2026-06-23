use corepc_client::bitcoin::{Address, Amount, Txid};
use corepc_node::{Conf, Node};
use observer::clients::rpc_client::RpcClient;
use observer::infra::config::RpcConfig;

pub fn setup_node() -> Node {
    let conf = Conf::default();
    match corepc_node::exe_path() {
        Ok(exe) => Node::with_conf(exe, &conf).expect("failed to start local bitcoind"),
        Err(_) => {
            Node::from_downloaded_with_conf(&conf).expect("failed to download/start bitcoind")
        }
    }
}

pub fn setup_node_and_rpc_client() -> (Node, RpcClient) {
    let node = setup_node();
    let rpc = setup_rpc_client(&node);
    (node, rpc)
}

pub fn rpc_config(node: &Node) -> RpcConfig {
    let cookie = node
        .params
        .get_cookie_values()
        .expect("read cookie file")
        .expect("cookie has user:pass");

    RpcConfig {
        host: node.params.rpc_socket.to_string(),
        user: cookie.user,
        pass: cookie.password,
    }
}

fn setup_rpc_client(node: &Node) -> RpcClient {
    RpcClient::new(&rpc_config(node)).expect("init rpc client")
}

pub fn maturate_coinbase(node: &Node, address: &Address) {
    node.client
        .generate_to_address(101, address)
        .expect("mine 101 blocks");
}

pub fn send_to_address(node: &Node, address: &Address) -> Txid {
    node.client
        .send_to_address(address, Amount::from_btc(1.0).unwrap())
        .expect("send to address")
        .txid()
        .expect("extract txid from send_to_address result")
}
