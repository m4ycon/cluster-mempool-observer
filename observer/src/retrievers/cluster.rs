use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use corepc_client::bitcoin::Txid;
use shared::models::{GetMempoolClusterModel, GetMempoolClusterRaw};
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

        let response: GetMempoolClusterRaw = self
            .rpc
            .call(move |client| {
                // TODO: change this raw call when new release of corepc is updated
                // (current 0.15), available implementation has a parsing bug
                client.call("getmempoolcluster", &[txid.to_string().into()])
            })
            .await?;

        Ok(GetMempoolClusterModel::from(&response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::models::ClusterChunkRaw;

    fn chunk(txids: &[&str], fee_sat: u64) -> ClusterChunkRaw {
        ClusterChunkRaw {
            // the RPC reports chunkfee in BTC
            chunkfee: fee_sat as f64 / 1_0000_0000.0,
            txs: txids.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn getmempoolcluster_model_flattens_txids_and_sums_fees() {
        let response = GetMempoolClusterRaw {
            clusterweight: 1600,
            txcount: 3,
            chunks: vec![chunk(&["a", "b"], 1000), chunk(&["c"], 500)],
        };

        let model = GetMempoolClusterModel::from(&response);

        assert_eq!(model.cluster_weight, 1600);
        assert_eq!(model.tx_count, 3);
        assert_eq!(
            model.txids,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(model.total_fee_sats, 1500);
    }
}
