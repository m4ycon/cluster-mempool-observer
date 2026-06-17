use crate::subjects::Subject;
use futures::Stream;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

const CHANNEL_CAPACITY: usize = 1024;

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
        let message = Message {
            subject: subject.as_str().to_string(),
            payload,
        };
        // An error only means there are no current subscribers — fine to drop.
        let _ = self.channels[&subject].send(message);
    }

    pub async fn subscribe(&self, subject: Subject) -> impl Stream<Item = Message> {
        // Drop `Lagged` errors from slow consumers; yield only delivered messages.
        BroadcastStream::new(self.channels[&subject].subscribe()).filter_map(|res| res.ok())
    }
}

impl Default for PubSub {
    fn default() -> Self {
        Self::new()
    }
}
