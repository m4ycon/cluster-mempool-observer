use corepc_node::Node;
use observer::infra::config::{Config, RpcConfig, WatchersConfig, ZmqConfig};

pub fn get_config_with_zmq_blocks(node: &Node, blocks_endpoint: String) -> Config {
    let mut config = get_config_with_rpc_config(node);
    config.zmq.blocks_endpoint = Some(blocks_endpoint);
    config.watchers.block = true;
    config.watchers.mempool_delta = false;
    config
}

pub fn get_config_with_rpc_config(node: &Node) -> Config {
    // TODO: way of enabling specific watchers with just one method?
    Config {
        rpc: RpcConfig {
            host: node.params.rpc_socket.to_string(),
            user: String::new(),
            pass: String::new(),
        },
        zmq: ZmqConfig::default(),
        poll_interval_secs: 1,
        log_level: "debug".to_string(),
        watchers: WatchersConfig {
            mempool_delta: true,
            block: false,
        },
    }
}
