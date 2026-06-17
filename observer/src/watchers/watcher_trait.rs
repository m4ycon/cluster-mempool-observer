use crate::error::ObserverError;
use crate::infra::config::WatchersConfig;
use crate::publisher::publish_event;
use serde::Serialize;
use shared::pubsub::PubSub;
use shared::subjects::Subject;
use std::fmt::Debug;
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub trait Watcher: Send {
    type Response: Send;
    type Event: Debug + Serialize + Send + Sync;

    /// Polls the source. `Some(response)` when the data changed (publish it),
    /// `None` when unchanged. The impl owns its dedup/delta state.
    fn watch(
        &mut self,
    ) -> impl Future<Output = Result<Option<Self::Response>, ObserverError>> + Send;

    /// Maps a response to the event published.
    fn to_event(&self, response: &Self::Response) -> Self::Event;

    /// Poll interval, in seconds.
    fn get_watch_rate(&self) -> u32;

    /// Subject the event is published to.
    fn get_publish_subject(&self) -> Subject;

    /// Whether the watcher is enabled by config.
    fn is_enabled(&self, config: &WatchersConfig) -> bool;

    /// Polls at the watcher's rate, publishing the event whenever the
    /// data changed. Runs forever; spawn it on its own task.
    fn run(mut self, pubsub: PubSub) -> impl Future<Output = ()> + Send
    where
        Self: Sized + 'static,
    {
        async move {
            let rate = Duration::from_secs(self.get_watch_rate() as u64);
            let subject = self.get_publish_subject();
            loop {
                let start = Instant::now();
                match self.watch().await {
                    Ok(Some(response)) => {
                        publish_event(&pubsub, subject, &self.to_event(&response)).await
                    }
                    Ok(None) => {}
                    Err(e) => tracing::error!("watcher error on {subject}: {e:?}"),
                }

                let elapsed = start.elapsed();
                if elapsed < rate {
                    sleep(rate - elapsed).await;
                }
            }
        }
    }
}
