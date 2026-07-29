use serde::Serialize;
use shared::pubsub::PubSub;
use shared::subjects::Subject;
use std::fmt::Debug;

/// Events dropped before reaching the bus.
const EVENT_SERIALIZE_ERRORS_TOTAL: &str = "event_serialize_errors_total";

pub async fn publish_event<Event: Serialize + Debug>(
    pubsub: &PubSub,
    subject: Subject,
    event: &Event,
) {
    // TODO: json for now, but if needed this can be changed to something else, like protobuf
    let payload = match serde_json::to_vec(event) {
        Ok(bytes) => bytes,
        Err(e) => {
            metrics::counter!(EVENT_SERIALIZE_ERRORS_TOTAL, "subject" => subject.as_str())
                .increment(1);
            tracing::error!("Failed to serialize event {event:?}: {e}");
            return;
        }
    };

    tracing::info!("Publishing event to {subject}: {event:?}");

    pubsub.publish(subject, payload).await;
}
