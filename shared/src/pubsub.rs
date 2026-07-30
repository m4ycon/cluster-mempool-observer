use crate::subjects::Subject;
use futures::Stream;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

const CHANNEL_CAPACITY: usize = 1024;

/// Messages handed to the bus, whether or not anyone was listening.
const PUBSUB_PUBLISHED_TOTAL: &str = "pubsub_published_total";

/// Messages a subscriber fell too far behind to receive. They are dropped, not
/// replayed, so this is the only record that the data existed.
const PUBSUB_LAGGED_TOTAL: &str = "pubsub_lagged_total";

/// Messages still held for the slowest subscriber.
const PUBSUB_QUEUE_DEPTH: &str = "pubsub_queue_depth";

/// Live receivers per subject -- the fanout each publish pays for.
const PUBSUB_SUBSCRIBERS: &str = "pubsub_subscribers";

/// A message delivered by the bus. Mirrors the subject+bytes shape consumers
/// relied on from NATS.
#[derive(Debug, Clone)]
pub struct Message {
    pub subject: String,
    pub payload: Vec<u8>,
}

/// In-process pub-sub backed by one tokio broadcast channel per subject.
/// Cheap to clone (shared `Arc`). Stand-in for the eventual tokio-channel design.
#[derive(Clone)]
pub struct PubSub {
    channels: Arc<HashMap<Subject, broadcast::Sender<Message>>>,
}

impl PubSub {
    pub fn new() -> Self {
        let channels = Subject::ALL
            .iter()
            .map(|&subject| (subject, broadcast::channel(CHANNEL_CAPACITY).0))
            .collect();
        Self {
            channels: Arc::new(channels),
        }
    }

    pub async fn publish(&self, subject: Subject, payload: Vec<u8>) {
        let label = subject.as_str();
        let message = Message {
            subject: label.to_string(),
            payload,
        };

        metrics::counter!(PUBSUB_PUBLISHED_TOTAL, "subject" => label).increment(1);
        // An error only means there are no current subscribers
        let _ = self.channels[&subject].send(message);
    }

    /// One occupancy reading for every subject.
    pub fn sample(&self) {
        for (subject, channel) in self.channels.iter() {
            let label = subject.as_str();
            metrics::gauge!(PUBSUB_SUBSCRIBERS, "subject" => label)
                .set(channel.receiver_count() as f64);
            metrics::gauge!(PUBSUB_QUEUE_DEPTH, "subject" => label).set(channel.len() as f64);
        }
    }

    pub async fn subscribe(&self, subject: Subject) -> impl Stream<Item = Message> + use<> {
        let label = subject.as_str();
        BroadcastStream::new(self.channels[&subject].subscribe()).filter_map(move |res| match res {
            Ok(message) => Some(message),
            Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                metrics::counter!(PUBSUB_LAGGED_TOTAL, "subject" => label).increment(skipped);
                tracing::warn!("subscriber lagged on {label}, dropped {skipped} messages");
                None
            }
        })
    }
}

impl Default for PubSub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod metrics_tests {
    use super::*;
    use testkit::metrics::{assert_no_series, assert_series, capture};

    const MEMPOOL: &str = r#"subject="rpc.mempooldelta""#;

    fn publish_n(bus: &PubSub, subject: Subject, count: usize) -> impl Future<Output = ()> + use<> {
        let bus = bus.clone();
        async move {
            for i in 0..count {
                bus.publish(subject, vec![i as u8]).await;
            }
        }
    }

    #[test]
    fn published_total_counts_every_message() {
        let rendered = capture(async {
            let bus = PubSub::new();
            publish_n(&bus, Subject::MempoolDelta, 3).await;
        });
        assert_series(&rendered, &format!("pubsub_published_total{{{MEMPOOL}}} 3"));
    }

    #[test]
    fn published_total_counts_messages_with_no_subscribers() {
        let rendered = capture(async {
            let bus = PubSub::new();
            bus.publish(Subject::MempoolDelta, vec![1]).await;
            bus.sample();
        });
        assert_series(&rendered, &format!("pubsub_published_total{{{MEMPOOL}}} 1"));
        assert_series(&rendered, &format!("pubsub_subscribers{{{MEMPOOL}}} 0"));
    }

    #[test]
    fn sample_reports_every_subject_even_idle_subjects() {
        let rendered = capture(async {
            PubSub::new().sample();
        });
        for subject in Subject::ALL {
            assert_series(
                &rendered,
                &format!(r#"pubsub_queue_depth{{subject="{}"}} 0"#, subject.as_str()),
            );
            assert_series(
                &rendered,
                &format!(r#"pubsub_subscribers{{subject="{}"}} 0"#, subject.as_str()),
            );
        }
    }

    #[test]
    fn subscribers_are_seen_without_any_traffic() {
        let rendered = capture(async {
            let bus = PubSub::new();
            let _sub = bus.subscribe(Subject::MempoolDelta).await;
            bus.sample();
        });
        assert_series(&rendered, &format!("pubsub_subscribers{{{MEMPOOL}}} 1"));
        assert_no_series(&rendered, "pubsub_published_total");
    }

    #[test]
    fn subscribers_gauge_tracks_live_receivers() {
        let rendered = capture(async {
            let bus = PubSub::new();
            let _a = bus.subscribe(Subject::MempoolDelta).await;
            let _b = bus.subscribe(Subject::MempoolDelta).await;
            bus.sample();
        });
        assert_series(&rendered, &format!("pubsub_subscribers{{{MEMPOOL}}} 2"));
    }

    #[test]
    fn subjects_do_not_share_series() {
        let rendered = capture(async {
            let bus = PubSub::new();
            publish_n(&bus, Subject::MempoolDelta, 2).await;
            bus.publish(Subject::BlockConnected, vec![1]).await;
        });
        assert_series(&rendered, &format!("pubsub_published_total{{{MEMPOOL}}} 2"));
        assert_series(
            &rendered,
            r#"pubsub_published_total{subject="zmq.blockconnected"} 1"#,
        );
    }

    #[test]
    fn queue_depth_tracks_the_unread_backlog() {
        let rendered = capture(async {
            let bus = PubSub::new();
            let _idle = bus.subscribe(Subject::MempoolDelta).await;
            publish_n(&bus, Subject::MempoolDelta, 4).await;
            bus.sample();
        });
        assert_series(&rendered, &format!("pubsub_queue_depth{{{MEMPOOL}}} 4"));
    }

    /// Nothing is retained once every subscriber is gone, so depth returns to
    /// zero rather than growing forever.
    #[test]
    fn queue_depth_is_zero_without_subscribers() {
        let rendered = capture(async {
            let bus = PubSub::new();
            publish_n(&bus, Subject::MempoolDelta, 4).await;
            bus.sample();
        });
        assert_series(&rendered, &format!("pubsub_queue_depth{{{MEMPOOL}}} 0"));
    }

    #[test]
    fn lagged_total_counts_messages_a_slow_subscriber_missed() {
        let overflow = 5;
        let rendered = capture(async {
            let bus = PubSub::new();
            let mut stream = Box::pin(bus.subscribe(Subject::MempoolDelta).await);
            publish_n(&bus, Subject::MempoolDelta, CHANNEL_CAPACITY + overflow).await;

            // Draining hits the lag error first, then the surviving messages.
            let first = stream.next().await.expect("stream yields after lagging");
            assert_eq!(first.payload, vec![overflow as u8]);
        });
        assert_series(
            &rendered,
            &format!("pubsub_lagged_total{{{MEMPOOL}}} {overflow}"),
        );
    }

    #[test]
    fn lagged_total_stays_absent_for_a_subscriber_that_keeps_up() {
        let rendered = capture(async {
            let bus = PubSub::new();
            let mut stream = Box::pin(bus.subscribe(Subject::MempoolDelta).await);
            bus.publish(Subject::MempoolDelta, vec![1]).await;
            stream.next().await.expect("message delivered");
        });
        assert_no_series(&rendered, "pubsub_lagged_total");
    }
}
