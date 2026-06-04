use std::fmt::Debug;

pub async fn publish_event<Event: Debug>(event: &Event) {
    tracing::info!("Publishing event: {:?}", event);
}
