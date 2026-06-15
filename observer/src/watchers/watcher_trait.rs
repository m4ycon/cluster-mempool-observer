use crate::error::ObserverError;
use crate::infra::config::WatchersConfig;
use serde::Serialize;
use serde::de::DeserializeOwned;
use shared::subjects::Subject;
use std::{fmt::Debug, time::Instant};

pub trait Watcher: Send {
    type Response: PartialEq + Send;
    type Event: Debug + Serialize + DeserializeOwned + Send + Sync;

    /// The event's subject from this watcher are published to.
    fn subject(&self) -> Subject;

    /// Watches the source for a new response
    fn watch(&mut self) -> impl Future<Output = Result<Self::Response, ObserverError>> + Send;

    /// Says if the watcher can watch again, the watcher can have an
    /// internal state that makes it unable to watch again.
    fn can_watch_again(&self) -> bool;

    /// Updates the last response, returns true if the response
    /// is different from the last one.
    fn update_last_response(&mut self, response: &Self::Response) -> bool;

    /// Transforms the response into an event. Usually will reduce the response
    /// to a smaller set of data, or transform it into a different format.
    fn to_event(&self, response: &Self::Response) -> Self::Event;

    /// Says if the watcher is enabled by config
    fn is_enabled(&self, config: &WatchersConfig) -> bool;

    /// Executes the watcher default workflow, watching for data, checking if
    /// it changed, and publishing the resulting event if so.
    fn run_once<F, Fut>(&mut self, publish_event: F) -> impl Future<Output = ()> + Send
    where
        F: FnOnce(Subject, Self::Event) -> Fut + Send,
        Fut: Future<Output = ()> + Send,
    {
        async move {
            if !self.can_watch_again() {
                return;
            }

            let start = Instant::now();
            let res = match self.watch().await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Error watching: {:?}", e);
                    return;
                }
            };
            tracing::debug!(
                "{} watcher call took {:?}",
                self.subject(),
                start.elapsed()
            );

            if !self.update_last_response(&res) {
                return;
            }

            let subject = self.subject();
            let event = self.to_event(&res);

            let start = Instant::now();
            publish_event(subject, event).await;
            tracing::debug!("{} publish took {:?}", self.subject(), start.elapsed());
        }
    }
}
