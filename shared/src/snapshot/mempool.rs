use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// Live, shareable snapshot of the mempool-delta watcher's tracked txid set.
#[derive(Clone, Default)]
pub struct MempoolSnapshot {
    inner: Arc<RwLock<HashSet<String>>>,
}

impl MempoolSnapshot {
    pub fn store(&self, txids: HashSet<String>) {
        *self.inner.write().expect("mempool snapshot poisoned") = txids;
    }

    pub fn get(&self) -> HashSet<String> {
        self.inner
            .read()
            .expect("mempool snapshot poisoned")
            .clone()
    }
}
