use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use shared::models::GetBlockchainInfoModel;
use std::future::Future;

pub trait ChainRetriever: Clone + Send + Sync {
    /// Chain state via `getblockchaininfo`.
    fn get_blockchain_info(
        &self,
    ) -> impl Future<Output = Result<GetBlockchainInfoModel, ObserverError>> + Send;
}

#[derive(Clone)]
pub struct ChainRpcRetriever {
    rpc: RpcClient,
}

impl ChainRpcRetriever {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }
}

impl ChainRetriever for ChainRpcRetriever {
    async fn get_blockchain_info(&self) -> Result<GetBlockchainInfoModel, ObserverError> {
        let info = self
            .rpc
            .call("getblockchaininfo", |client| client.get_blockchain_info())
            .await?;

        Ok(GetBlockchainInfoModel {
            blocks: info.blocks,
            headers: info.headers,
            verification_progress: info.verification_progress,
            initial_block_download: info.initial_block_download,
        })
    }
}
