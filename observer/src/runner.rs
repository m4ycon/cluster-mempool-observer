use crate::{
    clients::Clients,
    infra::config::Config,
    watchers::{getrawmempool::GetRawMempoolWatcher, watcher_trait::Watcher},
};

pub async fn run(config: &Config, clients: Clients) {
    let watcher = GetRawMempoolWatcher::new(clients.rpc.clone(), config.poll_interval_secs as u32);
    if watcher.is_enabled(&config.watchers) {
        tokio::spawn(watcher.run(clients.pubsub.clone()));
    }
    // future watchers: construct + is_enabled + spawn watcher.run(...), each with its own rate
}
