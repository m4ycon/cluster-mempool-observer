use async_nats::SubscribeError;
use futures::{Stream, StreamExt};
use serde::de::DeserializeOwned;
use shared::nats::NatsConfig;
use shared::subjects::Subject;
use std::io;

#[derive(Clone)]
pub struct NatsService {
    client: async_nats::Client,
}

impl NatsService {
    pub async fn connect(config: &NatsConfig) -> Result<Self, io::Error> {
        let client = shared::nats::connect(config).await?;
        Ok(Self { client })
    }

    pub async fn subscribe<T: DeserializeOwned>(
        &self,
        subject: Subject,
    ) -> Result<impl Stream<Item = T>, SubscribeError> {
        let sub = self.client.subscribe(subject.as_str()).await?;
        Ok(sub.filter_map(|msg| async move {
            match serde_json::from_slice::<T>(&msg.payload) {
                Ok(event) => Some(event),
                Err(e) => {
                    tracing::warn!("Ignoring malformed payload on {}: {e}", msg.subject);
                    None
                }
            }
        }))
    }
}
