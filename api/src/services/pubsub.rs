use futures::{Stream, StreamExt};
use serde::de::DeserializeOwned;
use shared::pubsub::PubSub;
use shared::subjects::Subject;

#[derive(Clone)]
pub struct PubSubService {
    client: PubSub,
}

impl PubSubService {
    pub fn new(client: PubSub) -> Self {
        Self { client }
    }

    pub async fn subscribe<T: DeserializeOwned>(&self, subject: Subject) -> impl Stream<Item = T> {
        let sub = self.client.subscribe(subject).await;
        sub.filter_map(|msg| async move {
            match serde_json::from_slice::<T>(&msg.payload) {
                Ok(event) => Some(event),
                Err(e) => {
                    tracing::warn!("Ignoring malformed payload on {}: {e}", msg.subject);
                    None
                }
            }
        })
    }
}
