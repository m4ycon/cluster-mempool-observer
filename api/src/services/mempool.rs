use crate::db::models::{NewMempoolDelta, NewTransaction};
use crate::db::{MempoolDeltaRepository, TransactionRepository};
use crate::services::cluster::ClusterService;
use crate::services::pubsub::PubSubService;
use futures::{Stream, StreamExt, stream};
use observer::retrievers::{
    ClusterRetriever, ClusterRpcRetriever, TransactionRetriever, TransactionRpcRetriever,
};
use shared::events::MempoolDeltaEvent;
use shared::subjects::Subject;
use std::collections::HashMap;

const MAX_CONCURRENT_TXS_INSERTS: usize = 4;

#[derive(Clone)]
pub struct MempoolService<
    TR: TransactionRetriever = TransactionRpcRetriever,
    CR: ClusterRetriever = ClusterRpcRetriever,
> {
    mempool_delta_repository: MempoolDeltaRepository,
    transaction_repository: TransactionRepository,
    transaction_retriever: TR,
    cluster_service: ClusterService<CR>,
    pubsub: PubSubService,
}

impl<TR: TransactionRetriever, CR: ClusterRetriever> MempoolService<TR, CR> {
    pub fn new(
        mempool_delta_repository: MempoolDeltaRepository,
        transaction_repository: TransactionRepository,
        transaction_retriever: TR,
        cluster_service: ClusterService<CR>,
        pubsub: PubSubService,
    ) -> Self {
        Self {
            mempool_delta_repository,
            transaction_repository,
            transaction_retriever,
            cluster_service,
            pubsub,
        }
    }

    pub async fn get_delta_stream(&self) -> impl Stream<Item = MempoolDeltaEvent> + use<TR, CR> {
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
        // TODO: fee not available from getrawtransaction non-verbose
        self.persist_delta_and_txs(delta, &HashMap::new()).await;
    }

    pub async fn apply_bootstrap_delta(
        &self,
        delta: MempoolDeltaEvent,
        fees: HashMap<String, i64>,
    ) {
        self.persist_delta_and_txs(delta, &fees).await;
    }

    async fn persist_delta_and_txs(&self, delta: MempoolDeltaEvent, fees: &HashMap<String, i64>) {
        let added = delta.added.clone();

        if let Err(e) = self
            .mempool_delta_repository
            .insert(&NewMempoolDelta::from(&delta))
            .await
        {
            tracing::error!("failed to persist mempool delta: {e}");
        }

        let existing_txids = match self.transaction_repository.existing_txids(&added).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("failed to check existing transactions: {e}");
                return;
            }
        };
        let new_txids = added
            .clone()
            .into_iter()
            .filter(|txid| !existing_txids.contains(txid))
            .collect::<Vec<_>>();

        // fetch and persist new transactions concurrently
        stream::iter(new_txids)
            .map(async |txid| {
                let client = self.transaction_retriever.clone();
                let mut new_tx = match client.get_raw_transaction(&txid).await {
                    Ok(tx) => NewTransaction::from(&tx),
                    Err(e) => {
                        tracing::error!("failed to retrieve transaction, persisting hollow: {e:?}");
                        NewTransaction::hollow(&txid)
                    }
                };
                new_tx.fee = fees.get(&txid).copied();

                if let Err(e) = self.transaction_repository.insert(&new_tx).await {
                    tracing::error!("failed to persist transaction: {e}");
                }
            })
            .buffer_unordered(MAX_CONCURRENT_TXS_INSERTS)
            .collect::<Vec<_>>()
            .await;

        // fetch and persist the clusters the new txs belong to
        self.cluster_service.sync_clusters_for(&added).await;
    }
}
