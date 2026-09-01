use crate::TS_EXPORT_DIR;
use crate::models::GetBlockchainInfoModel;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use time::OffsetDateTime;
use ts_rs::TS;

/// A delta of the get_raw_mempool between two consecutive polls
#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct MempoolDeltaEvent {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl std::fmt::Debug for MempoolDeltaEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MempoolDeltaEvent {{ added: {}, removed: {} }}",
            self.added.len(),
            self.removed.len()
        )
    }
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct ClusterRef {
    #[ts(type = "number")]
    pub id: i64,
    pub txids: Vec<String>,
    #[ts(type = "number")]
    pub total_vsize: i64,
    #[ts(type = "number")]
    pub total_fee: i64,
    #[serde(with = "time::serde::rfc3339")]
    #[ts(type = "string")]
    pub first_seen_at: OffsetDateTime,
}

#[derive(Serialize, Deserialize, Default, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct ClusterDeltaEvent {
    pub upserted: Vec<ClusterRef>,
    pub removed: Vec<i64>,
}

impl std::fmt::Debug for ClusterDeltaEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ClusterDeltaEvent {{ upserted: {}, removed: {} }}",
            self.upserted.len(),
            self.removed.len()
        )
    }
}

/// A block connected to the chain tip (via ZMQ `hashblock`)
#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct BlockConnectedEvent {
    pub hash: String,
}

impl std::fmt::Debug for BlockConnectedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlockConnectedEvent {{ hash: {} }}", self.hash)
    }
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct MempoolStatsEvent {
    #[ts(type = "number")]
    pub mempool_size: i64,
    #[ts(type = "number")]
    pub cluster_count: i64,
    #[ts(type = "number")]
    pub tx_per_min: i64,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct NewBlockInfoEvent {
    #[ts(type = "number")]
    pub height: i64,
    #[serde(with = "time::serde::rfc3339")]
    #[ts(type = "string")]
    pub mined_at: OffsetDateTime,
}

/// Node reachability as observed by one poll of `getblockchaininfo`.
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub enum NodeStatusEvent {
    Reachable {
        #[ts(type = "number")]
        blocks: i64,
        #[ts(type = "number")]
        headers: i64,
        verification_progress: f64,
        initial_block_download: bool,
    },
    Unreachable {
        error: String,
    },
}

impl NodeStatusEvent {
    pub fn reachable(info: &GetBlockchainInfoModel) -> Self {
        Self::Reachable {
            blocks: info.blocks,
            headers: info.headers,
            verification_progress: info.verification_progress,
            initial_block_download: info.initial_block_download,
        }
    }

    pub fn unreachable(error: impl Display) -> Self {
        Self::Unreachable {
            error: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_status_reachable_carries_the_chain_fields() {
        let info = GetBlockchainInfoModel {
            blocks: 100,
            headers: 102,
            verification_progress: 0.99,
            initial_block_download: false,
        };

        let event = NodeStatusEvent::reachable(&info);

        match event {
            NodeStatusEvent::Reachable {
                blocks,
                headers,
                verification_progress,
                initial_block_download,
            } => {
                assert_eq!(blocks, 100);
                assert_eq!(headers, 102);
                assert_eq!(verification_progress, 0.99);
                assert!(!initial_block_download);
            }
            NodeStatusEvent::Unreachable { .. } => panic!("expected Reachable"),
        }
    }

    #[test]
    fn node_status_unreachable_carries_only_the_error() {
        let event = NodeStatusEvent::unreachable("connection refused");

        match event {
            NodeStatusEvent::Unreachable { error } => assert_eq!(error, "connection refused"),
            NodeStatusEvent::Reachable { .. } => panic!("expected Unreachable"),
        }
    }
}
