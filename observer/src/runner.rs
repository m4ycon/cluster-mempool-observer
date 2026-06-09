use std::{
    future::Future,
    pin::Pin,
    time::{Duration, Instant},
};

use futures::future::join_all;
use tokio::time::sleep;

use crate::{
    extractors::{extractor_trait::Extractor, getrawmempool::GetRawMempoolExtractor},
    infra::config::{Config, ExtractorsConfig},
    publisher::publish_event,
};

pub async fn run(config: Config) {
    let mut extractors: Vec<Box<dyn DynExtractor>> =
        vec![Box::new(GetRawMempoolExtractor::default()) as Box<dyn DynExtractor>]
            .into_iter()
            .filter(|e| e.is_enabled(&config.extractors))
            .collect();

    let poll_interval = Duration::from_secs(config.poll_interval_secs);

    loop {
        let start = Instant::now();

        let futures = extractors.iter_mut().map(|e| e.run_once());
        join_all(futures).await;

        let elapsed = start.elapsed();
        if elapsed < poll_interval {
            sleep(poll_interval - elapsed).await;
        }
    }
}

/// Object-safe wrapper over [`Extractor`]. Hides the associated `Response` /
/// `Event` types (and the `impl Future` from `extract`) behind a single
/// `&mut self` method so extractors can be stored as `Box<dyn DynExtractor>`.
trait DynExtractor: Send {
    fn run_once<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
    fn is_enabled(&self, config: &ExtractorsConfig) -> bool;
}

impl<T: Extractor> DynExtractor for T
where
    T::Event: 'static,
{
    fn run_once<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(Extractor::run_once(self, |subject, event| async move {
            publish_event(subject, &event).await
        }))
    }

    fn is_enabled(&self, config: &ExtractorsConfig) -> bool {
        Extractor::is_enabled(self, config)
    }
}
