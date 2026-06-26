use serde::{Deserialize, Serialize};

/// A delta of the get_raw_mempool between two consecutive polls
#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize, Clone)]
pub struct ClusterRef {
    pub id: i64,
    pub txids: Vec<String>,
    pub total_fee: i64,
}

#[derive(Serialize, Deserialize, Default)]
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
#[derive(Serialize, Deserialize, Clone)]
pub struct BlockConnectedEvent {
    pub hash: String,
}

impl std::fmt::Debug for BlockConnectedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlockConnectedEvent {{ hash: {} }}", self.hash)
    }
}
