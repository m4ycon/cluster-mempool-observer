use async_nats::Client;
use serde::Serialize;
use shared::subjects::Subject;
use std::fmt::Debug;

pub async fn publish_event<Event: Serialize + Debug>(
    nats: &Client,
    subject: Subject,
    event: &Event,
) {
    // TODO: json for now, but if needed this can be changed to something else, like protobuf
    let payload = match serde_json::to_vec(event) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Failed to serialize event {event:?}: {e}");
            return;
        }
    };

    tracing::info!("Publishing event to {subject}: {event:?}");

    if let Err(e) = nats.publish(subject.as_str(), payload.into()).await {
        tracing::error!("Failed to publish event to {subject}: {e}");
    }
}
