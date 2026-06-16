use corepc_node::Node;
use observer::infra::config::{Config, RetrieversConfig, RpcConfig, WatchersConfig};
use shared::nats::NatsConfig;

pub fn get_config_with_rpc_config(node: &Node) -> Config {
    // TODO: way of enabling specific watchers with just one method?
    Config {
        rpc: RpcConfig {
            host: node.params.rpc_socket.to_string(),
            user: String::new(),
            pass: String::new(),
        },
        poll_interval_secs: 1,
        log_level: "debug".to_string(),
        nats: NatsConfig::default(),
        watchers: WatchersConfig {
            getrawmempool: true,
        },
        retrievers: RetrieversConfig {
            getrawtransaction: true,
            getrawmempoolverbose: true,
        },
    }
}
