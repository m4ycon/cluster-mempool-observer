use crate::db::MempoolDeltaRepository;
use crate::db::models::NewMempoolDelta;
use crate::services::pubsub::PubSubService;
use futures::{Stream, StreamExt};
use shared::events::MempoolDeltaEvent;
use shared::subjects::Subject;

pub async fn mempool_delta_stream(
    pubsub: &PubSubService,
) -> impl Stream<Item = MempoolDeltaEvent> + use<> {
    pubsub
        .subscribe::<MempoolDeltaEvent>(Subject::MempoolDelta)
        .await
}

pub async fn persist_deltas<S>(repo: MempoolDeltaRepository, stream: S)
where
    S: Stream<Item = MempoolDeltaEvent>,
{
    let mut stream = std::pin::pin!(stream);
    while let Some(delta) = stream.next().await {
        if let Err(e) = repo.insert(&NewMempoolDelta::from(&delta)).await {
            tracing::error!("failed to persist mempool delta: {e}");
        }
    }
}
