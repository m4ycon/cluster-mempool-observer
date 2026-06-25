use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use corepc_client::bitcoin::BlockHash;
use shared::models::GetBlockModel;
use std::future::Future;

pub trait BlockRetriever: Clone + Send + Sync {
    /// Fetches a mined block by hash via `getblock <hash> 2`.
    fn get_block(
        &self,
        hash: &str,
    ) -> impl Future<Output = Result<GetBlockModel, ObserverError>> + Send;
}

#[derive(Clone)]
pub struct BlockRpcRetriever {
    rpc: RpcClient,
}

impl BlockRpcRetriever {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }
}

impl BlockRetriever for BlockRpcRetriever {
    async fn get_block(&self, hash: &str) -> Result<GetBlockModel, ObserverError> {
        let hash = hash
            .parse::<BlockHash>()
            .map_err(|e| ObserverError::InvalidParams(e.to_string()))?;

        let response = self
            .rpc
            .call(move |client| client.get_block_verbose_two(hash))
            .await?;

        Ok(GetBlockModel::from(&response))
    }
}
