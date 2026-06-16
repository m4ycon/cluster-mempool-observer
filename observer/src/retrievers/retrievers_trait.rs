use crate::error::ObserverError;
use crate::infra::config::RetrieversConfig;
use crate::publisher::publish_event;
use async_nats::Client;
use futures::StreamExt;
use serde::{Serialize, de::DeserializeOwned};
use shared::subjects::Subject;
use std::fmt::Debug;

pub trait Retriever: Send {
    type Params: DeserializeOwned + Send;
    type Response: PartialEq + Send;
    type Event: Debug + Serialize + DeserializeOwned + Send + Sync;

    /// The client that events are sent through.
    fn publisher(&self) -> &Client;

    /// The subject the resulting event is published to.
    fn publish_subject(&self) -> Subject;

    /// The subject to subscribe to for triggering the retrieval.
    fn subscribe_subject(&self) -> Subject;

    /// Says if the retriever is enabled by config
    fn is_enabled(&self, config: &RetrieversConfig) -> bool;

    /// Transforms the response into an event.
    fn to_event(response: &Self::Response) -> Self::Event;

    /// Retrieves data from the source given the parameters.
    fn retrieve(
        &mut self,
        params: Self::Params,
    ) -> impl Future<Output = Result<Self::Response, ObserverError>> + Send;

    /// Subscribes to a subject and, for each request, retrieves the data
    /// and publishes the resulting event to another subject.
    /// Runs until the subscription closes.
    fn run(&mut self) -> impl Future<Output = ()> + Send {
        async move {
            let nats = self.publisher().clone();
            let subscribe_subject = self.subscribe_subject();
            let mut subscriber = match nats.subscribe(subscribe_subject.as_str()).await {
                Ok(subscriber) => subscriber,
                Err(e) => {
                    tracing::error!("Failed to subscribe to {subscribe_subject}: {e}");
                    return;
                }
            };
            tracing::info!("Retriever listening on {subscribe_subject}");

            while let Some(message) = subscriber.next().await {
                let params: Self::Params = match serde_json::from_slice(&message.payload) {
                    Ok(params) => params,
                    Err(e) => {
                        tracing::error!("Failed to deserialize params on {subscribe_subject}: {e}");
                        continue;
                    }
                };

                let response = match self.retrieve(params).await {
                    Ok(response) => response,
                    Err(e) => {
                        tracing::error!("Failed to retrieve on {subscribe_subject}: {e:?}");
                        continue;
                    }
                };

                let event = Self::to_event(&response);
                publish_event(&nats, self.publish_subject(), &event).await;
            }

            tracing::warn!("Subscription on {subscribe_subject} closed, retriever stopping");
        }
    }
}
