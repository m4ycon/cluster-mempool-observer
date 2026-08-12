use crate::{
    clients::Clients,
    infra::config::Config,
    watchers::{
        block::BlockWatcher,
        feerate_diagram::FeerateDiagramWatcher,
        mempool_delta::MempoolDeltaWatcher,
        watcher_trait::{WatcherRPC, WatcherZMQ},
    },
};
use shared::snapshot::{FeerateDiagramSnapshot, MempoolSnapshot};

pub async fn run(
    config: &Config,
    clients: Clients,
    mempool_snapshot: MempoolSnapshot,
    feerate_diagram_snapshot: FeerateDiagramSnapshot,
) {
    let mempool_delta = MempoolDeltaWatcher::new(
        clients.rpc.clone(),
        config.poll_interval_secs as u32,
        mempool_snapshot,
    );
    tokio::spawn(mempool_delta.run(clients.pubsub.clone()));

    let feerate_diagram = FeerateDiagramWatcher::new(
        clients.rpc.clone(),
        config.poll_interval_secs as u32,
        feerate_diagram_snapshot,
    );
    tokio::spawn(feerate_diagram.run(clients.pubsub.clone()));

    let block = BlockWatcher::new(clients.zmq.clone());
    tokio::spawn(block.run(clients.pubsub.clone()));
}
