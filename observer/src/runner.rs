use crate::{
    clients::Clients,
    infra::config::Config,
    watchers::{
        block::BlockWatcher,
        mempool_delta::MempoolDeltaWatcher,
        watcher_trait::{WatcherRPC, WatcherZMQ},
    },
};
use shared::snapshot::MempoolSnapshot;

pub async fn run(config: &Config, clients: Clients, snapshot: MempoolSnapshot) {
    let mempool_delta = MempoolDeltaWatcher::new(
        clients.rpc.clone(),
        config.poll_interval_secs as u32,
        snapshot,
    );
    tokio::spawn(mempool_delta.run(clients.pubsub.clone()));

    let block = BlockWatcher::new(clients.zmq.clone());
    tokio::spawn(block.run(clients.pubsub.clone()));
}
