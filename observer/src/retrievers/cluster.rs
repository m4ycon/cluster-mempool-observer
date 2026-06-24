use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use corepc_client::bitcoin::Txid;
use shared::models::GetMempoolClusterModel;
use std::future::Future;

pub trait ClusterRetriever: Clone + Send + Sync {
    /// Fetches the cluster a transaction belongs to via `getmempoolcluster`.
    fn get_mempool_cluster(
        &self,
        txid: &str,
    ) -> impl Future<Output = Result<GetMempoolClusterModel, ObserverError>> + Send;
}

#[derive(Clone)]
pub struct ClusterRpcRetriever {
    rpc: RpcClient,
}

impl ClusterRpcRetriever {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }
}

impl ClusterRetriever for ClusterRpcRetriever {
    async fn get_mempool_cluster(
        &self,
        txid: &str,
    ) -> Result<GetMempoolClusterModel, ObserverError> {
        let txid = txid
            .parse::<Txid>()
            .map_err(|e| ObserverError::InvalidParams(e.to_string()))?;

        let response = self
            .rpc
            .call(move |client| client.get_mempool_cluster(txid))
            .await?
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))?;

        Ok(GetMempoolClusterModel::from(&response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corepc_client::bitcoin::hashes::Hash;
    use corepc_client::bitcoin::{Amount, Txid};
    use corepc_client::types::model::{Chunk, GetMempoolCluster};

    fn chunk(seeds: &[[u8; 32]], fee_sat: u64) -> Chunk {
        Chunk {
            chunk_fee: Amount::from_sat(fee_sat),
            chunk_weight: 400,
            txs: seeds.iter().map(|s| Txid::from_byte_array(*s)).collect(),
        }
    }

    #[test]
    fn getmempoolcluster_model_flattens_txids_and_sums_fees() {
        let response = GetMempoolCluster {
            cluster_weight: 1600,
            tx_count: 3,
            chunks: vec![
                chunk(&[[1u8; 32], [2u8; 32]], 1000),
                chunk(&[[3u8; 32]], 500),
            ],
        };

        let model = GetMempoolClusterModel::from(&response);

        let expected_txids: Vec<String> = [[1u8; 32], [2u8; 32], [3u8; 32]]
            .iter()
            .map(|s| Txid::from_byte_array(*s).to_string())
            .collect();
        assert_eq!(model.cluster_weight, 1600);
        assert_eq!(model.tx_count, 3);
        assert_eq!(model.txids, expected_txids);
        assert_eq!(model.total_fee_sats, 1500);
    }
}
