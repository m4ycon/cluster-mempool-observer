pub use bitcoind::BitcoinD;

use bitcoind::Conf;
use corepc_client::bitcoin::{Address, Amount, Txid};
use observer::clients::rpc_client::RpcClient;
use observer::infra::config::RpcConfig;
use std::net::{SocketAddr, TcpListener};

fn start_node(conf: &Conf) -> BitcoinD {
    match bitcoind::exe_path() {
        Ok(exe) => BitcoinD::with_conf(exe, conf).expect("failed to start local bitcoind"),
        Err(_) => {
            BitcoinD::from_downloaded_with_conf(conf).expect("failed to download/start bitcoind")
        }
    }
}

pub fn setup_node() -> BitcoinD {
    setup_node_with_args(&[])
}

pub fn setup_node_with_args(args: &[&str]) -> BitcoinD {
    let mut conf = Conf::default();
    conf.args.extend_from_slice(args);
    start_node(&conf)
}

pub fn setup_node_and_rpc_client() -> (BitcoinD, RpcClient) {
    let node = setup_node();
    let rpc = setup_rpc_client(&node);
    (node, rpc)
}

pub fn setup_node_with_zmq_hashblock() -> (BitcoinD, RpcClient, String) {
    let port = find_free_port();
    let hashblock_arg = format!("-zmqpubhashblock=tcp://0.0.0.0:{port}");

    let node = setup_node_with_args(&[&hashblock_arg]);

    let rpc = setup_rpc_client(&node);
    let endpoint = format!("tcp://127.0.0.1:{port}");
    (node, rpc, endpoint)
}

pub fn rpc_config(node: &BitcoinD) -> RpcConfig {
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

/// The node's credentials, aimed at `addr` instead of the node itself.
pub fn rpc_config_via(node: &BitcoinD, addr: SocketAddr) -> RpcConfig {
    RpcConfig {
        host: addr.to_string(),
        ..rpc_config(node)
    }
}

fn setup_rpc_client(node: &BitcoinD) -> RpcClient {
    RpcClient::new(&rpc_config(node)).expect("init rpc client")
}

pub fn maturate_coinbase(node: &BitcoinD, address: &Address) {
    node.client
        .generate_to_address(101, address)
        .expect("mine 101 blocks");
}

pub fn send_to_address(node: &BitcoinD, address: &Address) -> Txid {
    node.client
        .send_to_address(address, Amount::from_btc(1.0).unwrap())
        .expect("send to address")
        .txid()
        .expect("extract txid from send_to_address result")
}

fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("read local addr")
        .port()
}

#[cfg(test)]
mod tests {
    #[test]
    fn regtest_bitcoind_matches_the_compose_node_image() {
        let compose = include_str!("../../compose.yml");
        let image_tag = compose
            .lines()
            .find_map(|line| line.trim().strip_prefix("image: bitcoin/bitcoin:"))
            .expect("compose.yml declares a bitcoin/bitcoin image");

        assert_eq!(
            image_tag,
            bitcoind::VERSION,
            "compose.yml runs Bitcoin Core {image_tag} but the integration tests run {}; \
             move the bitcoind feature in testkit/Cargo.toml along with the image",
            bitcoind::VERSION
        );
    }
}
