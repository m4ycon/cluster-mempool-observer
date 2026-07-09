use crate::{
    clients::Clients,
    infra::config::Config,
    watchers::{
        block::BlockWatcher,
        mempool_delta::MempoolDeltaWatcher,
        watcher_trait::{Watcher, WatcherRPC, WatcherZMQ},
    },
};
use shared::snapshot::MempoolSnapshot;

pub async fn run(config: &Config, clients: Clients, snapshot: MempoolSnapshot) {
    let watcher = MempoolDeltaWatcher::new(
        clients.rpc.clone(),
        config.poll_interval_secs as u32,
        snapshot,
    );
    if watcher.is_enabled(&config.watchers) {
        tokio::spawn(watcher.run(clients.pubsub.clone()));
    }

    let watcher = BlockWatcher::new(clients.zmq.clone());
    if watcher.is_enabled(&config.watchers) {
        tokio::spawn(watcher.run(clients.pubsub.clone()));
    }
}
