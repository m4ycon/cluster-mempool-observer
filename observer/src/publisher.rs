use serde::Serialize;
use shared::pubsub::PubSub;
use shared::subjects::Subject;
use std::fmt::Debug;

pub async fn publish_event<Event: Serialize + Debug>(
    pubsub: &PubSub,
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

    pubsub.publish(subject, payload).await;
}
