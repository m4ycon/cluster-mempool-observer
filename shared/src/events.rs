use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use ts_rs::TS;

/// Where ts-rs writes the generated TypeScript bindings. Relative to ts-rs'
/// default export dir (`shared/bindings/`), so `../../web/...` lands at the repo
/// root `web/src/types/generated/`.
const TS_EXPORT_DIR: &str = "../../web/src/types/generated/";

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

#[derive(Serialize, Deserialize, Debug, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub enum HomeMessage {
    Stats(MempoolStatsEvent),
    Block(NewBlockInfoEvent),
}
