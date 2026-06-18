use crate::services::pubsub::PubSubService;
use futures::Stream;
use shared::events::MempoolDeltaEvent;
use shared::subjects::Subject;

pub async fn mempool_delta_stream(pubsub: &PubSubService) -> impl Stream<Item = MempoolDeltaEvent> {
    pubsub
        .subscribe::<MempoolDeltaEvent>(Subject::MempoolDelta)
        .await
}
