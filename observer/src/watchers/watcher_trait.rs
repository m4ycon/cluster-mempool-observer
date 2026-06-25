use crate::error::ObserverError;
use crate::infra::config::WatchersConfig;
use crate::publisher::publish_event;
use futures::StreamExt;
use serde::Serialize;
use shared::pubsub::PubSub;
use shared::subjects::Subject;
use std::fmt::Debug;
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub trait Watcher: Send {
    type Event: Debug + Serialize + Send + Sync;

    /// Subject the event is published to.
    fn get_publish_subject(&self) -> Subject;

    /// Whether the watcher is enabled by config.
    fn is_enabled(&self, config: &WatchersConfig) -> bool;
}

pub trait WatcherRPC: Watcher {
    type Response: Send;

    /// Polls the source. `Some(response)` when the data changed (publish it),
    /// `None` when unchanged. The impl owns its dedup/delta state.
    fn watch(
        &mut self,
    ) -> impl Future<Output = Result<Option<Self::Response>, ObserverError>> + Send;

    /// Poll interval, in seconds.
    fn get_watch_rate(&self) -> u32;

    /// Maps a response to the event published.
    fn to_event(&self, response: &Self::Response) -> Self::Event;

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

pub trait WatcherZMQ: Watcher {
    fn get_stream(&self) -> Result<bitcoincore_zmq::MessageStream, ObserverError>;

    fn handle_message(&self, msg: bitcoincore_zmq::Message) -> Result<Self::Event, ObserverError>;

    fn run(self, pubsub: PubSub) -> impl Future<Output = ()> + Send
    where
        Self: Sized + 'static,
    {
        const TRY_RECONNECT_AFTER: Duration = Duration::from_secs(5);

        async move {
            loop {
                let mut stream = match self.get_stream() {
                    Ok(stream) => stream,
                    Err(e) => {
                        tracing::error!("block watcher: zmq blocks subscribe failed: {e:?}");
                        sleep(TRY_RECONNECT_AFTER).await;
                        continue;
                    }
                };
                tracing::info!("block watcher: subscribed to zmq blocks");

                while let Some(msg) = stream.next().await {
                    match msg {
                        Ok(msg) => {
                            let event = match self.handle_message(msg) {
                                Ok(event) => event,
                                Err(e) => {
                                    tracing::error!(
                                        "block watcher: failed to handle zmq message: {e:?}"
                                    );
                                    continue;
                                }
                            };
                            publish_event(&pubsub, self.get_publish_subject(), &event).await;
                        }
                        Err(e) => tracing::error!("block watcher: zmq decode error: {e}"),
                    }
                }

                tracing::warn!("block watcher: zmq stream ended, reconnecting");
                sleep(TRY_RECONNECT_AFTER).await;
            }
        }
    }
}
