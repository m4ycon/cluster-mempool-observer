use crate::services::pubsub::PubSubService;
use futures::Stream;
use shared::events::GetRawMempoolEvent;
use shared::subjects::Subject;

pub async fn rawmempool_stream(pubsub: &PubSubService) -> impl Stream<Item = GetRawMempoolEvent> {
    pubsub.subscribe::<GetRawMempoolEvent>(Subject::RawMempool).await
}
