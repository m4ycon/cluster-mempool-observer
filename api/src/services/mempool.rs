use crate::services::nats::NatsService;
use async_nats::SubscribeError;
use futures::Stream;
use shared::events::GetRawMempoolEvent;
use shared::subjects::Subject;

pub async fn rawmempool_stream(
    nats: &NatsService,
) -> Result<impl Stream<Item = GetRawMempoolEvent>, SubscribeError> {
    nats.subscribe::<GetRawMempoolEvent>(Subject::RawMempool)
        .await
}
