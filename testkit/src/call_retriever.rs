use futures::{Stream, StreamExt, pin_mut};
use serde::Serialize;
use serde::de::DeserializeOwned;
use shared::pubsub::{Message, PubSub};
use shared::subjects::Subject;
use std::time::Duration;

/// Overall budget for a retriever call to be answered.
const CALL_TIMEOUT: Duration = Duration::from_secs(1);
/// How often the call is re-published while waiting for an answer.
const RESPONSE_INTERVAL: Duration = Duration::from_millis(300);

pub struct RetrieverCall<'a, P, S> {
    pub subscriber: S,
    pub request_subject: Subject,
    pub request_params: &'a P,
}

/// Publishes `call.request_params` (JSON-encoded) to `call.request_subject` and waits for the
/// answer on `call.subscriber`, returning it deserialized.
pub async fn call_retriever<P, R, S>(pubsub: &PubSub, call: RetrieverCall<'_, P, S>) -> R
where
    P: Serialize,
    R: DeserializeOwned,
    S: Stream<Item = Message>,
{
    let RetrieverCall {
        subscriber,
        request_subject,
        request_params,
    } = call;
    pin_mut!(subscriber);

    let payload = serde_json::to_vec(request_params).expect("serialize retriever params");

    let message = tokio::time::timeout(CALL_TIMEOUT, async {
        loop {
            pubsub.publish(request_subject, payload.clone()).await;

            if let Ok(Some(message)) =
                tokio::time::timeout(RESPONSE_INTERVAL, subscriber.next()).await
            {
                break message;
            }
        }
    })
    .await
    .expect("retriever call answered within timeout");

    serde_json::from_slice(&message.payload).expect("deserialize answer payload")
}
