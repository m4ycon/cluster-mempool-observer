use crate::db::models::{NewMempoolDelta, NewTransaction};
use crate::db::{MempoolDeltaRepository, TransactionRepository};
use crate::services::pubsub::PubSubService;
use futures::{Stream, StreamExt, stream};
use observer::retrievers::TransactionRetriever;
use shared::events::MempoolDeltaEvent;
use shared::subjects::Subject;

const MAX_CONCURRENT_TXS_INSERTS: usize = 4;

pub async fn mempool_delta_stream(
    pubsub: &PubSubService,
) -> impl Stream<Item = MempoolDeltaEvent> + use<> {
    pubsub
        .subscribe::<MempoolDeltaEvent>(Subject::MempoolDelta)
        .await
}

pub async fn persist_deltas_and_new_txs<S, R>(
    mempool_repo: MempoolDeltaRepository,
    transaction_repo: TransactionRepository,
    transaction_retriever: R,
    stream: S,
) where
    S: Stream<Item = MempoolDeltaEvent>,
    R: TransactionRetriever + 'static,
{
    let mut stream = std::pin::pin!(stream);
    while let Some(delta) = stream.next().await {
        if let Err(e) = mempool_repo.insert(&NewMempoolDelta::from(&delta)).await {
            tracing::error!("failed to persist mempool delta: {e}");
        }

        let existing_txids = match transaction_repo.existing_txids(&delta.added).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("failed to check existing transactions: {e}");
                continue;
            }
        };
        let new_txids = delta
            .added
            .into_iter()
            .filter(|txid| !existing_txids.contains(txid))
            .collect::<Vec<_>>();

        // fetch and persist new transactions concurrently
        stream::iter(new_txids)
            .map(async |txid| {
                let client = transaction_retriever.clone();
                let tx = match client.get_raw_transaction(&txid).await {
                    Ok(tx) => tx,
                    Err(e) => return tracing::error!("failed to retrieve transaction: {e:?}"),
                };

                if let Err(e) = transaction_repo.insert(&NewTransaction::from(tx)).await {
                    tracing::error!("failed to persist transaction: {e}");
                }
            })
            .buffer_unordered(MAX_CONCURRENT_TXS_INSERTS)
            .collect::<Vec<_>>()
            .await;
    }
}
