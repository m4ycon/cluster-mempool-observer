use crate::{
    infra::config::{Config, WatchersConfig},
    publisher::publish_event,
    watchers::{getrawmempool::GetRawMempoolWatcher, watcher_trait::Watcher},
};
use futures::future::join_all;
use std::{
    future::Future,
    pin::Pin,
    time::{Duration, Instant},
};
use tokio::time::sleep;

pub async fn run(config: Config) {
    let mut watchers: Vec<Box<dyn DynWatcher>> =
        vec![Box::new(GetRawMempoolWatcher::default()) as Box<dyn DynWatcher>]
            .into_iter()
            .filter(|e| e.is_enabled(&config.watchers))
            .collect();

    let poll_interval = Duration::from_secs(config.poll_interval_secs);

    loop {
        let start = Instant::now();

        let futures = watchers.iter_mut().map(|e| e.run_once());
        join_all(futures).await;

        let elapsed = start.elapsed();
        if elapsed < poll_interval {
            sleep(poll_interval - elapsed).await;
        }
    }
}

/// Object-safe wrapper over [`Watcher`]. Hides the associated `Response` /
/// `Event` types (and the `impl Future` from `watch`) behind a single
/// `&mut self` method so watchers can be stored as `Box<dyn DynWatcher>`.
trait DynWatcher: Send {
    fn run_once<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
    fn is_enabled(&self, config: &WatchersConfig) -> bool;
}

impl<T: Watcher> DynWatcher for T
where
    T::Event: 'static,
{
    fn run_once<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(Watcher::run_once(self, |subject, event| async move {
            publish_event(subject, &event).await
        }))
    }

    fn is_enabled(&self, config: &WatchersConfig) -> bool {
        Watcher::is_enabled(self, config)
    }
}
