use crate::clients::rpc_client;
use crate::error::ObserverError;
use crate::infra::config::RetrieversConfig;
use crate::retrievers::retrievers_trait::Retriever;
use corepc_client::types::model::GetRawMempoolVerbose;
use shared::events::{GetRawMempoolVerboseEvent, MempoolEntrySummary};
use shared::subjects::Subject;

#[derive(Default)]
pub struct GetRawMempoolVerboseRetriever;

impl Retriever for GetRawMempoolVerboseRetriever {
    type Params = ();
    type Response = GetRawMempoolVerbose;
    type Event = GetRawMempoolVerboseEvent;

    fn publish_subject(&self) -> Subject {
        Subject::RawMempoolVerbose
    }

    fn subscribe_subject(&self) -> Subject {
        Subject::RequestRawMempoolVerbose
    }

    fn is_enabled(&self, config: &RetrieversConfig) -> bool {
        config.getrawmempoolverbose
    }

    async fn retrieve(&mut self, _params: Self::Params) -> Result<Self::Response, ObserverError> {
        let response = rpc_client::get()
            .call(|client| client.get_raw_mempool_verbose())
            .await?;

        response
            .into_model()
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))
    }

    fn to_event(&self, response: &Self::Response) -> Self::Event {
        let entries = response
            .0
            .iter()
            .map(|(txid, entry)| MempoolEntrySummary {
                txid: txid.to_string(),
                fee_in_sats: entry.fees.base.to_sat(),
                vsize: entry.vsize.unwrap_or_default(),
                ancestor_count: entry.ancestor_count,
                descendant_count: entry.descendant_count,
                time: entry.time,
                height: entry.height,
            })
            .collect();

        Self::Event { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corepc_client::bitcoin::hashes::Hash;
    use corepc_client::bitcoin::{Amount, Txid, Wtxid};
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
    fn getrawmempoolverbose_event_maps_entry_fields() {
        let response = dummy_response([7u8; 32], 1234);
        let event = GetRawMempoolVerboseRetriever.to_event(&response);

        assert_eq!(event.entries.len(), 1);
        let summary = &event.entries[0];
        assert_eq!(summary.txid, Txid::from_byte_array([7u8; 32]).to_string());
        assert_eq!(summary.fee_in_sats, 1234);
        assert_eq!(summary.vsize, 140);
        assert_eq!(summary.ancestor_count, 2);
        assert_eq!(summary.descendant_count, 1);
        assert_eq!(summary.time, 1_700_000_000);
        assert_eq!(summary.height, 800_000);
    }
}
