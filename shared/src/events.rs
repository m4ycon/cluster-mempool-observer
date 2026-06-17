use serde::{Deserialize, Serialize};

/// A delta of the get_raw_mempool between two consecutive polls
#[derive(Serialize, Deserialize)]
pub struct GetRawMempoolEvent {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl std::fmt::Debug for GetRawMempoolEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GetRawMempoolEvent {{ added: {}, removed: {} }}",
            self.added.len(),
            self.removed.len()
        )
    }
}
