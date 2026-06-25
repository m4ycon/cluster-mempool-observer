use crate::db::models::{NewBlock, NewTransaction};
use crate::db::{BlockRepository, TransactionRepository};
use crate::services::cluster::ClusterService;
use crate::services::pubsub::PubSubService;
use futures::{Stream, StreamExt};
use observer::retrievers::{
    BlockRetriever, BlockRpcRetriever, ClusterRetriever, ClusterRpcRetriever,
};
use shared::events::BlockConnectedEvent;
use shared::subjects::Subject;
use std::collections::HashMap;

#[derive(Clone)]
pub struct BlockService<
    BR: BlockRetriever = BlockRpcRetriever,
    CR: ClusterRetriever = ClusterRpcRetriever,
> {
    block_repository: BlockRepository,
    transaction_repository: TransactionRepository,
    cluster_service: ClusterService<CR>,
    block_retriever: BR,
    pubsub: PubSubService,
}

impl<BR: BlockRetriever, CR: ClusterRetriever> BlockService<BR, CR> {
    pub fn new(
        block_repository: BlockRepository,
        transaction_repository: TransactionRepository,
        cluster_service: ClusterService<CR>,
        block_retriever: BR,
        pubsub: PubSubService,
    ) -> Self {
        Self {
            block_repository,
            transaction_repository,
            cluster_service,
            block_retriever,
            pubsub,
        }
    }

    pub async fn get_block_stream(&self) -> impl Stream<Item = BlockConnectedEvent> + use<BR, CR> {
        self.pubsub
            .subscribe::<BlockConnectedEvent>(Subject::BlockConnected)
            .await
    }

    pub async fn persist_blocks_and_txs<S>(&self, stream: S)
    where
        S: Stream<Item = BlockConnectedEvent>,
    {
        let mut stream = std::pin::pin!(stream);
        while let Some(event) = stream.next().await {
            self.apply_block(event).await;
        }
    }

    pub async fn apply_block(&self, event: BlockConnectedEvent) {
        let block = match self.block_retriever.get_block(&event.hash).await {
            Ok(block) => block,
            Err(e) => {
                tracing::error!("failed to retrieve block {}: {e:?}", event.hash);
                return;
            }
        };

        if let Err(e) = self.block_repository.insert(&NewBlock::from(&block)).await {
            tracing::error!("failed to persist block {}: {e}", block.hash);
        }

        let confirmed_at = block.mined_at;
        let txids: Vec<String> = block.txs.iter().map(|tx| tx.txid.clone()).collect();
        let fees: HashMap<String, i64> = block
            .txs
            .iter()
            .map(|tx| (tx.txid.clone(), tx.fee_sats))
            .collect();

        let new_txs: Vec<NewTransaction> = block
            .txs
            .iter()
            .map(|tx| NewTransaction {
                txid: tx.txid.clone(),
                fee: Some(tx.fee_sats),
                vsize: tx.vsize,
                first_seen_at: confirmed_at,
                confirmed_at: Some(confirmed_at),
                cluster_id: None,
            })
            .collect();
        if let Err(e) = self
            .transaction_repository
            .insert_or_confirm_many(&new_txs)
            .await
        {
            tracing::error!("failed to persist block transactions: {e}");
        }

        // confirm clusters those txs belonged to
        self.cluster_service
            .confirm_mined(&txids, &fees, confirmed_at)
            .await;
    }
}
