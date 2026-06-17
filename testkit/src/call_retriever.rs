// DISABLED reference: NATS removed in step 1 (placeholder pub-sub). Kept as a
// reference for restoring real-transport test helpers in step 4. `cfg(any())`
// (applied at the `pub mod` in lib.rs) is always false, so this never compiles.
use async_nats::{Client, Subscriber};
use futures::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use shared::subjects::Subject;
use std::time::Duration;

/// Overall budget for a retriever call to be answered.
const CALL_TIMEOUT: Duration = Duration::from_secs(1);
/// How often the call is re-published while waiting for an answer.
const RESPONSE_INTERVAL: Duration = Duration::from_millis(300);

pub struct RetrieverCall<'a, P> {
    pub subscriber: &'a mut Subscriber,
    pub request_subject: &'a Subject,
    pub request_params: &'a P,
}

/// Publishes `call.request_params` (JSON-encoded) to `call.request_subject` and waits for the
/// answer on `call.subscriber`, returning it deserialized.
pub async fn call_retriever<P, R>(nats: &Client, call: RetrieverCall<'_, P>) -> R
where
    P: Serialize,
    R: DeserializeOwned,
{
    let RetrieverCall {
        subscriber,
        request_subject,
        request_params,
    } = call;

    let payload = serde_json::to_vec(request_params).expect("serialize retriever params");

    let message = tokio::time::timeout(CALL_TIMEOUT, async {
        loop {
            nats.publish(request_subject.as_str(), payload.clone().into())
                .await
                .expect("publish retriever call");

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
