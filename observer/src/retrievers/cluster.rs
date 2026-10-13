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

        let response = match self
            .rpc
            .call("getmempoolcluster", move |client| {
                client.get_mempool_cluster(txid)
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

        GetMempoolClusterModel::try_from(response)
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corepc_client::bitcoin::hashes::Hash;
    use corepc_client::types::v31::{Chunk, GetMempoolCluster};
    use shared::models::ClusterChunk;

    fn txid(byte: u8) -> Txid {
        Txid::from_byte_array([byte; 32])
    }

    fn chunk(txids: &[Txid], fee_sat: u64, weight: u64) -> Chunk {
        Chunk {
            // the RPC reports chunkfee in BTC
            chunk_fee: fee_sat as f64 / 1_0000_0000.0,
            chunk_weight: weight,
            txs: txids.iter().map(Txid::to_string).collect(),
        }
    }

    fn strings(txids: &[Txid]) -> Vec<String> {
        txids.iter().map(Txid::to_string).collect()
    }

    #[test]
    fn getmempoolcluster_model_keeps_chunks_in_mining_order() {
        let (a, b, c) = (txid(1), txid(2), txid(3));
        let response = GetMempoolCluster {
            cluster_weight: 1600,
            tx_count: 3,
            chunks: vec![chunk(&[a, b], 1000, 1000), chunk(&[c], 500, 600)],
        };

        let model = GetMempoolClusterModel::try_from(response).unwrap();

        assert_eq!(model.cluster_weight, 1600);
        assert_eq!(model.tx_count, 3);
        assert_eq!(
            model.chunks,
            vec![
                ClusterChunk {
                    txids: strings(&[a, b]),
                    fee_sats: 1000,
                    weight: 1000,
                },
                ClusterChunk {
                    txids: strings(&[c]),
                    fee_sats: 500,
                    weight: 600,
                },
            ]
        );
        assert_eq!(
            model.txids().cloned().collect::<Vec<_>>(),
            strings(&[a, b, c])
        );
        assert_eq!(model.total_fee_sats(), 1500);
    }

    #[test]
    fn getmempoolcluster_model_fails_on_an_unparseable_chunk_fee_instead_of_zeroing_it() {
        let mut bad = chunk(&[txid(2)], 0, 400);
        bad.chunk_fee = -0.0001;
        let response = GetMempoolCluster {
            cluster_weight: 800,
            tx_count: 2,
            chunks: vec![chunk(&[txid(1)], 500, 400), bad],
        };

        assert!(GetMempoolClusterModel::try_from(response).is_err());
    }
}
