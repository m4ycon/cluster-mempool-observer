use crate::subjects::Subject;
use futures::Stream;

/// A message delivered by the bus. Mirrors the subject+bytes shape consumers
/// relied on from NATS.
#[derive(Debug, Clone)]
pub struct Message {
    pub subject: String,
    pub payload: Vec<u8>,
}

/// Placeholder pub-sub client. No-op stub standing in for a real transport
/// (was NATS, will become in-process tokio channels). Cheap to clone.
#[derive(Clone, Default)]
pub struct PubSub;

impl PubSub {
    pub fn new() -> Self {
        Self
    }

    pub async fn publish(&self, subject: Subject, payload: Vec<u8>) {
        tracing::debug!(
            "pubsub publish {subject} ({} bytes) [placeholder no-op]",
            payload.len()
        );
    }

    pub async fn subscribe(&self, subject: Subject) -> impl Stream<Item = Message> {
        tracing::debug!("pubsub subscribe {subject} [placeholder no-op]");
        futures::stream::empty()
    }
}
