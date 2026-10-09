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

        let response: GetMempoolClusterRaw = match self
            .rpc
            .call("getmempoolcluster", move |client| {
                // TODO: change this raw call when new release of corepc is updated
                // (current 0.15), available implementation has a parsing bug
                client.call("getmempoolcluster", &[txid.to_string().into()])
            })
            .await
        {
            Ok(cluster) => cluster,
            Err(e) => {
                if e.to_string().contains("Transaction not in mempool") {
                    return Err(ObserverError::TxNotFoundInMempool(e.to_string()));
                }
                return Err(ObserverError::Other(e.to_string()));
            }
        };

        GetMempoolClusterModel::try_from(&response)
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::models::{ClusterChunk, ClusterChunkRaw};

    fn chunk(txids: &[&str], fee_sat: u64, weight: u64) -> ClusterChunkRaw {
        ClusterChunkRaw {
            // the RPC reports chunkfee in BTC
            chunkfee: fee_sat as f64 / 1_0000_0000.0,
            chunkweight: weight,
            txs: txids.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn txids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn getmempoolcluster_model_keeps_chunks_in_mining_order() {
        let response = GetMempoolClusterRaw {
            clusterweight: 1600,
            txcount: 3,
            chunks: vec![chunk(&["a", "b"], 1000, 1000), chunk(&["c"], 500, 600)],
        };

        let model = GetMempoolClusterModel::try_from(&response).unwrap();

        assert_eq!(model.cluster_weight, 1600);
        assert_eq!(model.tx_count, 3);
        assert_eq!(
            model.chunks,
            vec![
                ClusterChunk {
                    txids: txids(&["a", "b"]),
                    fee_sats: 1000,
                    weight: 1000,
                },
                ClusterChunk {
                    txids: txids(&["c"]),
                    fee_sats: 500,
                    weight: 600,
                },
            ]
        );
        assert_eq!(
            model.txids().cloned().collect::<Vec<_>>(),
            txids(&["a", "b", "c"])
        );
        assert_eq!(model.total_fee_sats(), 1500);
    }

    #[test]
    fn getmempoolcluster_model_fails_on_an_unparseable_chunk_fee_instead_of_zeroing_it() {
        let mut bad = chunk(&["b"], 0, 400);
        bad.chunkfee = -0.0001;
        let response = GetMempoolClusterRaw {
            clusterweight: 800,
            txcount: 2,
            chunks: vec![chunk(&["a"], 500, 400), bad],
        };

        assert!(GetMempoolClusterModel::try_from(&response).is_err());
    }
}
