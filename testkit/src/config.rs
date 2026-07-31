use crate::node::rpc_config;
use corepc_node::Node;
use observer::infra::config::{Config, WatchersConfig, ZmqConfig};

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
        rpc: rpc_config(node),
        zmq: ZmqConfig::default(),
        poll_interval_secs: 1,
        watchers: WatchersConfig {
            mempool_delta: true,
            block: false,
        },
    }
}
