use crate::db::BlockRepository;
use crate::db::models::{NewBlock, NewTransaction};
use crate::error::BlockSyncError;
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
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::{Mutex, OwnedMutexGuard};

/// Backoff before the first catch-up retry; doubles per attempt up to
/// `MAX_CATCH_UP_BACKOFF`.
const CATCH_UP_BACKOFF_BASE: Duration = Duration::from_millis(250);

const MAX_CATCH_UP_BACKOFF: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct BlockService<
    BR: BlockRetriever = BlockRpcRetriever,
    CR: ClusterRetriever = ClusterRpcRetriever,
> {
    block_repository: BlockRepository,
    mempool_ledger: MempoolLedger,
    block_gate: BlockGate,
    apply_lock: Arc<Mutex<()>>,
    cluster_service: ClusterService<CR>,
    block_retriever: BR,
    pubsub: PubSubService,
}

impl<BR: BlockRetriever, CR: ClusterRetriever> BlockService<BR, CR> {
    pub fn new(
        block_repository: BlockRepository,
        mempool_ledger: MempoolLedger,
        block_gate: BlockGate,
        apply_lock: Arc<Mutex<()>>,
        cluster_service: ClusterService<CR>,
        block_retriever: BR,
        pubsub: PubSubService,
    ) -> Self {
        Self {
            block_repository,
            mempool_ledger,
            block_gate,
            apply_lock,
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
        while stream.next().await.is_some() {
            self.sync_missing_blocks().await;
        }
    }

    pub async fn sync_missing_blocks(&self) {
        let mut attempts: u32 = 0;
        while self.catch_up().await.is_err() {
            let backoff = catch_up_backoff(attempts);
            tracing::warn!("block sync: retrying in {backoff:?}");
            tokio::time::sleep(backoff).await;
            attempts = attempts.saturating_add(1);
        }
    }

    /// Applies every height between our highest block and the node's tip, in
    /// order, stopping at the first that fails.
    async fn catch_up(&self) -> Result<(), BlockSyncError> {
        // The chain can advance while we sync, so we re-read the tip
        // after each pass and keep going until we've caught up to the live tip
        loop {
            let tip = self
                .block_retriever
                .get_tip_height()
                .await
                .inspect_err(|e| tracing::error!("block sync: failed to get tip height: {e}"))?;
            let latest = self
                .block_repository
                .latest_height()
                .await
                .inspect_err(|e| {
                    tracing::error!("block sync: failed to read latest height: {e}")
                })?;
            let next = match latest {
                Some(height) => height + 1,
                None => tip, // empty DB, we start from the actual tip
            };

            if next > tip {
                return Ok(());
            }

            if next < tip {
                tracing::warn!(
                    "block sync: {} blocks behind the tip, backfilling heights {next}..={tip}",
                    tip - next + 1
                );
            }
            for height in next..=tip {
                let hash = self
                    .block_retriever
                    .get_block_hash(height as u64)
                    .await
                    .inspect_err(|e| {
                        tracing::error!("block sync: failed to get hash for height {height}: {e}")
                    })?;
                self.apply_block(BlockConnectedEvent { hash: hash.clone() })
                    .await
                    .inspect_err(|e| {
                        tracing::error!(
                            "block sync: failed to apply {hash} at height {height}: {e}"
                        )
                    })?;
            }
        }
    }

    /// Waits for an in-flight `apply_block` to finish. While the guard lives, no
    /// other can start.
    pub async fn stop_applying(&self) -> OwnedMutexGuard<()> {
        self.apply_lock.clone().lock_owned().await
    }

    pub async fn apply_block(&self, event: BlockConnectedEvent) -> Result<(), BlockSyncError> {
        let _applying = self.apply_lock.lock().await;
        // Held for the whole body, not just until confirmed_at lands.
        let _guard = self.block_gate.acquire().await;

        let block = self.block_retriever.get_block(&event.hash).await?;

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
        self.block_repository
            .insert_with_transactions(&NewBlock::from(&block), &new_txs)
            .await?;

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

        Ok(())
    }
}

/// Doubles per attempt up to the ceiling: 250ms, 500ms, 1s ... 10s.
fn catch_up_backoff(attempts: u32) -> Duration {
    let factor = 1u32.checked_shl(attempts).unwrap_or(u32::MAX);
    CATCH_UP_BACKOFF_BASE
        .saturating_mul(factor)
        .min(MAX_CATCH_UP_BACKOFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_up_backoff_doubles_then_caps() {
        assert_eq!(catch_up_backoff(0), Duration::from_millis(250));
        assert_eq!(catch_up_backoff(1), Duration::from_millis(500));
        assert_eq!(catch_up_backoff(5), Duration::from_secs(8));
        assert_eq!(catch_up_backoff(6), MAX_CATCH_UP_BACKOFF);
        assert_eq!(catch_up_backoff(u32::MAX), MAX_CATCH_UP_BACKOFF);
    }
}
