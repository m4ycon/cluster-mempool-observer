use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use shared::models::GetNetworkInfoModel;
use std::future::Future;

pub trait NetworkRetriever: Clone + Send + Sync {
    /// Node version info via `getnetworkinfo`.
    fn get_network_info(
        &self,
    ) -> impl Future<Output = Result<GetNetworkInfoModel, ObserverError>> + Send;
}

#[derive(Clone)]
pub struct NetworkRpcRetriever {
    rpc: RpcClient,
}

impl NetworkRpcRetriever {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }
}

impl NetworkRetriever for NetworkRpcRetriever {
    async fn get_network_info(&self) -> Result<GetNetworkInfoModel, ObserverError> {
        let info = self
            .rpc
            .call("getnetworkinfo", |client| client.get_network_info())
            .await?;
        let model = info
            .into_model()
            .map_err(|e| ObserverError::Other(e.to_string()))?;

        Ok(GetNetworkInfoModel {
            version: model.version,
            subversion: model.subversion,
        })
    }
}
