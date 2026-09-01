use crate::error::ObserverError;
use crate::publisher::publish_event;
use bitcoincore_zmq::{MonitorMessage, SocketEvent, SocketMessage};
use futures::StreamExt;
use serde::Serialize;
use shared::metrics::{record_duration, record_elapsed};
use shared::pubsub::PubSub;
use shared::subjects::Subject;
use std::fmt::Debug;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// One poll of an RPC watcher: fetch, diff, and publish if it changed.
const WATCHER_POLL_SECONDS: &str = "watcher_poll_seconds";

/// Polls that failed to fetch. The loop keeps running, so without this a node
/// that stopped answering looks the same as a mempool that stopped changing.
const WATCHER_POLL_ERRORS_TOTAL: &str = "watcher_poll_errors_total";

/// Polls that took at least the whole interval, leaving no time to sleep. The
/// watcher is no longer polling at its configured rate.
const WATCHER_POLL_OVERRUNS_TOTAL: &str = "watcher_poll_overruns_total";

/// Messages taken off the ZMQ stream, including ones that fail to decode.
const ZMQ_MESSAGES_TOTAL: &str = "zmq_messages_total";

/// Time to turn one ZMQ message into a published event.
const ZMQ_MESSAGE_HANDLE_SECONDS: &str = "zmq_message_handle_seconds";

/// Messages dropped, split by where they were lost: `decode` on the wire,
/// `handle` when the payload was not what the watcher expects.
const ZMQ_ERRORS_TOTAL: &str = "zmq_errors_total";

/// Times the socket dropped, the stream ended, or the subscription could not be
/// established. A climbing count means the node connection is flapping, which no
/// latency metric would show.
const ZMQ_RECONNECTS_TOTAL: &str = "zmq_reconnects_total";

pub trait Watcher: Send {
    type Event: Debug + Serialize + Send + Sync;

    /// Subject the event is published to.
    fn get_publish_subject(&self) -> Subject;
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

    /// One poll cycle, returning how long it took so the caller can subtract it
    /// from the sleep.
    fn poll_once(&mut self, pubsub: &PubSub) -> impl Future<Output = Duration> + Send
    where
        Self: Sized,
    {
        async move {
            let subject = self.get_publish_subject();
            let started = Instant::now();

            match self.watch().await {
                Ok(Some(response)) => {
                    publish_event(pubsub, subject, &self.to_event(&response)).await
                }
                Ok(None) => {}
                Err(e) => {
                    metrics::counter!(WATCHER_POLL_ERRORS_TOTAL, "subject" => subject.as_str())
                        .increment(1);
                    tracing::error!("watcher error on {subject}: {e:?}");
                }
            }

            let elapsed = started.elapsed();
            record_duration(
                WATCHER_POLL_SECONDS,
                &[("subject", subject.as_str())],
                elapsed,
            );
            elapsed
        }
    }

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
                let elapsed = self.poll_once(&pubsub).await;
                if elapsed < rate {
                    sleep(rate - elapsed).await;
                } else {
                    metrics::counter!(WATCHER_POLL_OVERRUNS_TOTAL, "subject" => subject.as_str())
                        .increment(1);
                }
            }
        }
    }
}

pub trait WatcherZMQ: Watcher {
    fn get_stream(
        &self,
    ) -> impl Future<
        Output = Result<
            bitcoincore_zmq::subscribe_async_monitor_stream::MessageStream,
            ObserverError,
        >,
    > + Send;

    fn handle_message(&self, msg: bitcoincore_zmq::Message) -> Result<Self::Event, ObserverError>;

    /// Reacts to a socket lifecycle event.
    fn handle_socket_event(&self, event: MonitorMessage) {
        let subject = self.get_publish_subject();
        match event.event {
            SocketEvent::Disconnected { .. } => {
                metrics::counter!(ZMQ_RECONNECTS_TOTAL, "subject" => subject.as_str()).increment(1);
                tracing::warn!(
                    "block watcher: zmq disconnected from {}, libzmq is retrying",
                    event.source_url
                );
            }
            // The first handshake is awaited during subscribe, so any later one
            // is a recovery.
            SocketEvent::HandshakeSucceeded => {
                tracing::info!("block watcher: zmq reconnected to {}", event.source_url);
            }
            _ => {}
        }
    }

    /// Decodes one message and publishes it.
    fn handle_one(
        &self,
        msg: bitcoincore_zmq::Message,
        pubsub: &PubSub,
    ) -> impl Future<Output = ()> + Send
    where
        Self: Sync,
    {
        async move {
            let subject = self.get_publish_subject();
            metrics::counter!(ZMQ_MESSAGES_TOTAL, "subject" => subject.as_str()).increment(1);
            let started = Instant::now();

            let event = match self.handle_message(msg) {
                Ok(event) => event,
                Err(e) => {
                    metrics::counter!(
                        ZMQ_ERRORS_TOTAL,
                        "subject" => subject.as_str(),
                        "kind" => "handle",
                    )
                    .increment(1);
                    tracing::error!("block watcher: failed to handle zmq message: {e:?}");
                    return;
                }
            };

            publish_event(pubsub, subject, &event).await;
            record_elapsed(
                ZMQ_MESSAGE_HANDLE_SECONDS,
                &[("subject", subject.as_str())],
                started,
            );
        }
    }

    fn run(self, pubsub: PubSub) -> impl Future<Output = ()> + Send
    where
        Self: Sized + Sync + 'static,
    {
        const TRY_RECONNECT_AFTER: Duration = Duration::from_secs(5);

        async move {
            let subject = self.get_publish_subject();
            loop {
                let mut stream = match self.get_stream().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        metrics::counter!(ZMQ_RECONNECTS_TOTAL, "subject" => subject.as_str())
                            .increment(1);
                        tracing::error!("block watcher: zmq blocks subscribe failed: {e}");
                        sleep(TRY_RECONNECT_AFTER).await;
                        continue;
                    }
                };
                tracing::info!("block watcher: subscribed to zmq blocks");

                while let Some(msg) = stream.next().await {
                    match msg {
                        Ok(SocketMessage::Message(msg)) => self.handle_one(msg, &pubsub).await,
                        Ok(SocketMessage::Event(event)) => self.handle_socket_event(event),
                        Err(e) => {
                            metrics::counter!(ZMQ_MESSAGES_TOTAL, "subject" => subject.as_str())
                                .increment(1);
                            metrics::counter!(
                                ZMQ_ERRORS_TOTAL,
                                "subject" => subject.as_str(),
                                "kind" => "decode",
                            )
                            .increment(1);
                            tracing::error!("block watcher: zmq decode error: {e}");
                        }
                    }
                }

                metrics::counter!(ZMQ_RECONNECTS_TOTAL, "subject" => subject.as_str()).increment(1);
                tracing::warn!("block watcher: zmq stream ended, reconnecting");
                sleep(TRY_RECONNECT_AFTER).await;
            }
        }
    }
}
