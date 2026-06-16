use crate::{
    clients::Clients,
    infra::config::{Config, RetrieversConfig, WatchersConfig},
    retrievers::{
        getrawmempoolverbose::GetRawMempoolVerboseRetriever,
        getrawtransaction::GetRawTransactionRetriever, retrievers_trait::Retriever,
    },
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
    spawn_retrievers(config, &clients);
    run_watchers(config, &clients).await
}

fn spawn_retrievers(config: &Config, clients: &Clients) {
    let retrievers: Vec<Box<dyn DynTask<RetrieversConfig>>> = vec![
        Box::new(GetRawTransactionRetriever::new(
            clients.nats.clone(),
            clients.rpc.clone(),
        )) as Box<dyn DynTask<RetrieversConfig>>,
        Box::new(GetRawMempoolVerboseRetriever::new(
            clients.nats.clone(),
            clients.rpc.clone(),
        )) as Box<dyn DynTask<RetrieversConfig>>,
    ]
    .into_iter()
    .filter(|r| r.is_enabled(&config.retrievers))
    .collect();

    for mut retriever in retrievers {
        tokio::spawn(async move { retriever.run().await });
    }
}

async fn run_watchers(config: &Config, clients: &Clients) {
    let mut watchers: Vec<Box<dyn DynTask<WatchersConfig>>> = vec![Box::new(
        GetRawMempoolWatcher::new(clients.nats.clone(), clients.rpc.clone()),
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

/// Object-safe wrapper over a [`Watcher`] or [`Retriever`]. Hides the associated
/// types (and the `impl Future` from `run`) behind a single `&mut self` method
/// so tasks can be stored as `Box<dyn DynTask<C>>`, where `C` is the config
/// slice that gates them.
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

impl<T: Retriever> DynTask<RetrieversConfig> for T {
    fn run<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(Retriever::run(self))
    }

    fn is_enabled(&self, config: &RetrieversConfig) -> bool {
        Retriever::is_enabled(self, config)
    }
}
