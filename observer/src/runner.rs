use crate::{
    clients::Clients,
    infra::config::Config,
    snapshot::MempoolSnapshot,
    watchers::{mempool_delta::MempoolDeltaWatcher, watcher_trait::Watcher},
};

pub async fn run(config: &Config, clients: Clients, snapshot: MempoolSnapshot) {
    let watcher = MempoolDeltaWatcher::new(
        clients.rpc.clone(),
        config.poll_interval_secs as u32,
        snapshot,
    );
    if watcher.is_enabled(&config.watchers) {
        tokio::spawn(watcher.run(clients.pubsub.clone()));
    }
    // future watchers: construct + is_enabled + spawn watcher.run(...), each with its own rate
}
