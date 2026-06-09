use corepc_node::Node;
use observer::infra::config::{Config, ExtractorsConfig, NatsConfig, RpcConfig};

pub fn get_config_with_rpc_config(node: &Node) -> Config {
    // TODO: way of enabling specific extractors with just one method?
    Config {
        rpc: RpcConfig {
            host: node.params.rpc_socket.to_string(),
            user: String::new(),
            pass: String::new(),
        },
        poll_interval_secs: 1,
        log_level: "debug".to_string(),
        nats: NatsConfig::default(),
        extractors: ExtractorsConfig {
            getrawmempool: true,
        },
    }
}
