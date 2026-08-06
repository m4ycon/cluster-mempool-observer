use crate::node::rpc_config;
use corepc_node::Node;
use observer::infra::config::{Config, ZmqConfig};

pub const INERT_ZMQ_ENDPOINT: &str = "tcp://127.0.0.1:1";

pub fn get_config_with_zmq_blocks(node: &Node, blocks_endpoint: String) -> Config {
    let mut config = get_config_with_rpc_config(node);
    config.zmq.blocks_endpoint = blocks_endpoint;
    config
}

pub fn get_config_with_rpc_config(node: &Node) -> Config {
    Config {
        rpc: rpc_config(node),
        zmq: ZmqConfig {
            blocks_endpoint: INERT_ZMQ_ENDPOINT.into(),
        },
        poll_interval_secs: 1,
    }
}
