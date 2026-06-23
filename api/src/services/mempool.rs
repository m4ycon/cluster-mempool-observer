use crate::db::models::{NewMempoolDelta, NewTransaction};
use crate::db::{MempoolDeltaRepository, TransactionRepository};
use crate::services::pubsub::PubSubService;
use futures::{Stream, StreamExt, stream};
use observer::retrievers::TransactionRetriever;
use shared::events::MempoolDeltaEvent;
use shared::subjects::Subject;

const MAX_CONCURRENT_TXS_INSERTS: usize = 4;

#[derive(Clone)]
pub struct MempoolService<R: TransactionRetriever + Clone> {
    mempool_delta_repository: MempoolDeltaRepository,
    transaction_repository: TransactionRepository,
    transaction_retriever: R,
    pubsub: PubSubService,
}

impl<R: TransactionRetriever + Clone + 'static> MempoolService<R> {
    pub fn new(
        mempool_delta_repository: MempoolDeltaRepository,
        transaction_repository: TransactionRepository,
        transaction_retriever: R,
        pubsub: PubSubService,
    ) -> Self {
        Self {
            mempool_delta_repository,
            transaction_repository,
            transaction_retriever,
            pubsub,
        }
    }

    pub async fn get_delta_stream(&self) -> impl Stream<Item = MempoolDeltaEvent> + use<R> {
        self.pubsub
            .subscribe::<MempoolDeltaEvent>(Subject::MempoolDelta)
            .await
    }

    pub async fn persist_deltas_and_new_txs<S>(&self, stream: S)
    where
        S: Stream<Item = MempoolDeltaEvent>,
    {
        let mut stream = std::pin::pin!(stream);
        while let Some(delta) = stream.next().await {
            self.apply_delta(delta).await;
        }
    }

    /// Persists a single mempool delta and backfills any newly-seen transactions.
    pub async fn apply_delta(&self, delta: MempoolDeltaEvent) {
        if let Err(e) = self
            .mempool_delta_repository
            .insert(&NewMempoolDelta::from(&delta))
            .await
        {
            tracing::error!("failed to persist mempool delta: {e}");
        }

        let existing_txids = match self
            .transaction_repository
            .existing_txids(&delta.added)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("failed to check existing transactions: {e}");
                return;
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
                let client = self.transaction_retriever.clone();
                let tx = match client.get_raw_transaction(&txid).await {
                    Ok(tx) => tx,
                    Err(e) => return tracing::error!("failed to retrieve transaction: {e:?}"),
                };

                if let Err(e) = self
                    .transaction_repository
                    .insert(&NewTransaction::from(tx))
                    .await
                {
                    tracing::error!("failed to persist transaction: {e}");
                }
            })
            .buffer_unordered(MAX_CONCURRENT_TXS_INSERTS)
            .collect::<Vec<_>>()
            .await;
    }
}
