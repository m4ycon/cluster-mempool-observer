use crate::db::models::{NewBlock, NewTransaction};
use crate::db::{BlockRepository, TransactionRepository};
use crate::infra::block_gate::BlockGate;
use crate::services::cluster::ClusterService;
use crate::services::pubsub::PubSubService;
use futures::{Stream, StreamExt};
use observer::retrievers::{
    BlockRetriever, BlockRpcRetriever, ClusterRetriever, ClusterRpcRetriever,
};
use shared::events::{BlockConnectedEvent, NewBlockInfoEvent};
use shared::snapshot::MempoolLedger;
use shared::subjects::Subject;
use std::collections::HashMap;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct BlockService<
    BR: BlockRetriever = BlockRpcRetriever,
    CR: ClusterRetriever = ClusterRpcRetriever,
> {
    block_repository: BlockRepository,
    transaction_repository: TransactionRepository,
    mempool_ledger: MempoolLedger,
    block_gate: BlockGate,
    cluster_service: ClusterService<CR>,
    block_retriever: BR,
    pubsub: PubSubService,
}

impl<BR: BlockRetriever, CR: ClusterRetriever> BlockService<BR, CR> {
    pub fn new(
        block_repository: BlockRepository,
        transaction_repository: TransactionRepository,
        mempool_ledger: MempoolLedger,
        block_gate: BlockGate,
        cluster_service: ClusterService<CR>,
        block_retriever: BR,
        pubsub: PubSubService,
    ) -> Self {
        Self {
            block_repository,
            transaction_repository,
            mempool_ledger,
            block_gate,
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

    pub async fn sync_missing_blocks(&self) {
        // The chain can advance while we sync, so we re-read the tip
        // after each pass and keep going until we've caught up to the live tip
        loop {
            let tip = match self.block_retriever.get_tip_height().await {
                Ok(height) => height,
                Err(e) => {
                    tracing::error!("block sync: failed to get tip height: {e:?}");
                    return;
                }
            };

            let our_tip = match self.block_repository.latest_height().await {
                Ok(Some(height)) => height + 1, // next missing height
                Ok(None) => tip,                // empty DB, we start from the actual tip
                Err(e) => {
                    tracing::error!("block sync: failed to read latest height: {e}");
                    return;
                }
            };

            if our_tip > tip {
                return; // caught up with the live tip
            }

            tracing::info!("block sync: backfilling heights {our_tip}..={tip}");
            for height in our_tip..=tip {
                tracing::info!("block sync: backfilling height {height}");
                let hash = match self.block_retriever.get_block_hash(height as u64).await {
                    Ok(hash) => hash,
                    Err(e) => {
                        tracing::error!(
                            "block sync: failed to get hash for height {height}: {e:?}"
                        );
                        return;
                    }
                };
                self.apply_block(BlockConnectedEvent { hash }).await;
            }
        }
    }

    pub async fn apply_block(&self, event: BlockConnectedEvent) {
        // Held for the whole body, not just until confirmed_at lands.
        let _guard = self.block_gate.acquire();

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
        let sizes: HashMap<String, i64> = block
            .txs
            .iter()
            .map(|tx| (tx.txid.clone(), tx.vsize))
            .collect();

        let seen_at = OffsetDateTime::now_utc();
        let new_txs: Vec<NewTransaction> = block
            .txs
            .iter()
            .map(|tx| NewTransaction {
                txid: tx.txid.clone(),
                fee: Some(tx.fee_sats),
                vsize: tx.vsize,
                first_seen_at: seen_at,
                confirmed_at: Some(confirmed_at),
                cluster_id: None,
                confirmed_at_block: Some(block.hash.clone()),
                hollow: false,
                input_txids: Some(tx.input_txids.clone()),
            })
            .collect();
        if let Err(e) = self
            .transaction_repository
            .insert_or_confirm_many(&new_txs)
            .await
        {
            tracing::error!("failed to persist block transactions: {e}");
        }

        // remove_confirmed delta for each mined tx that was in our mempool
        self.mempool_ledger.assert_absent(&txids);

        // confirm clusters those txs belonged to
        self.cluster_service
            .confirm_mined(&txids, &fees, &sizes, confirmed_at)
            .await;

        // announce the new chain tip
        self.pubsub
            .publish(
                Subject::NewBlockInfo,
                &NewBlockInfoEvent {
                    height: block.height,
                    mined_at: block.mined_at,
                },
            )
            .await;
    }
}
