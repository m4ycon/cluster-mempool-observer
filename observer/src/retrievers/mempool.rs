use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use shared::models::GetRawMempoolVerboseModel;
use std::collections::HashSet;

/// On-demand mempool retrievals.
#[derive(Clone)]
pub struct MempoolRetriever {
    rpc: RpcClient,
}

impl MempoolRetriever {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }

    /// Fetches the full mempool via `getrawmempool` with verbose set to true.
    pub async fn get_raw_mempool_verbose(
        &self,
    ) -> Result<GetRawMempoolVerboseModel, ObserverError> {
        let response = self
            .rpc
            .call("getrawmempoolverbose", |client| {
                client.get_raw_mempool_verbose()
            })
            .await?
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))?;

        Ok(GetRawMempoolVerboseModel::from(&response))
    }

    /// Fetches the full mempool via `getrawmempool`.
    pub async fn get_mempool_txids(&self) -> Result<HashSet<String>, ObserverError> {
        let response = self
            .rpc
            .call("getrawmempool", |client| client.get_raw_mempool())
            .await?
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))?;

        Ok(response.0.iter().map(|txid| txid.to_string()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corepc_client::bitcoin::hashes::Hash;
    use corepc_client::bitcoin::{Amount, Txid, Wtxid};
    use corepc_client::types::model::GetRawMempoolVerbose;
    use corepc_client::types::model::{MempoolEntry, MempoolEntryFees};
    use std::collections::BTreeMap;

    fn dummy_response(seed: [u8; 32], base_sat: u64) -> GetRawMempoolVerbose {
        let entry = MempoolEntry {
            vsize: Some(140),
            size: None,
            weight: Some(560),
            time: 1_700_000_000,
            height: 800_000,
            descendant_count: 1,
            descendant_size: 140,
            ancestor_count: 2,
            ancestor_size: 280,
            wtxid: Wtxid::from_byte_array(seed),
            fees: MempoolEntryFees {
                base: Amount::from_sat(base_sat),
                modified: Amount::from_sat(base_sat),
                ancestor: Amount::from_sat(base_sat),
                descendant: Amount::from_sat(base_sat),
            },
            depends: vec![],
            spent_by: vec![],
            bip125_replaceable: Some(true),
            unbroadcast: Some(false),
        };

        let mut map = BTreeMap::new();
        map.insert(Txid::from_byte_array(seed), entry);
        GetRawMempoolVerbose(map)
    }

    #[test]
    fn getrawmempoolverbose_model_maps_entry_fields() {
        let response = dummy_response([7u8; 32], 1234);
        let model = GetRawMempoolVerboseModel::from(&response);

        assert_eq!(model.entries.len(), 1);
        let summary = &model.entries[0];
        assert_eq!(summary.txid, Txid::from_byte_array([7u8; 32]).to_string());
        assert_eq!(summary.fee_in_sats, 1234);
        assert_eq!(summary.vsize, 140);
        assert_eq!(summary.ancestor_count, 2);
        assert_eq!(summary.descendant_count, 1);
        assert_eq!(summary.time, 1_700_000_000);
        assert_eq!(summary.height, 800_000);
    }
}
