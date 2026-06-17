use crate::{
    clients::Clients,
    infra::config::{Config, WatchersConfig},
    watchers::{getrawmempool::GetRawMempoolWatcher, watcher_trait::Watcher},
};
use futures::future::join_all;
use std::{
    future::Future,
    pin::Pin,
    time::{Duration, Instant},
};
use tokio::time::sleep;

pub async fn run(config: &Config, clients: Clients) {
    run_watchers(config, &clients).await
}

async fn run_watchers(config: &Config, clients: &Clients) {
    let mut watchers: Vec<Box<dyn DynTask<WatchersConfig>>> = vec![Box::new(
        GetRawMempoolWatcher::new(clients.pubsub.clone(), clients.rpc.clone()),
    )
        as Box<dyn DynTask<WatchersConfig>>]
    .into_iter()
    .filter(|e| e.is_enabled(&config.watchers))
    .collect();

    let poll_interval = Duration::from_secs(config.poll_interval_secs);

    loop {
        let start = Instant::now();

        let futures = watchers.iter_mut().map(|e| e.run());
        join_all(futures).await;

        let elapsed = start.elapsed();
        if elapsed < poll_interval {
            sleep(poll_interval - elapsed).await;
        }
    }
}

// completely AI generated code below, to handle Vec<Box<dyn DynTask<C>>> the way I wanted

/// Object-safe wrapper over a [`Watcher`]. Hides the associated types (and the
/// `impl Future` from `run`) behind a single `&mut self` method so tasks can be
/// stored as `Box<dyn DynTask<C>>`, where `C` is the config slice that gates them.
trait DynTask<C>: Send {
    fn run<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
    fn is_enabled(&self, config: &C) -> bool;
}

impl<T: Watcher> DynTask<WatchersConfig> for T
where
    T::Event: 'static,
{
    fn run<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(Watcher::run(self))
    }

    fn is_enabled(&self, config: &WatchersConfig) -> bool {
        Watcher::is_enabled(self, config)
    }
}
