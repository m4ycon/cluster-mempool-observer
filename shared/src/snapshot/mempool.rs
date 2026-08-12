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

    pub fn len(&self) -> usize {
        self.inner.read().expect("mempool snapshot poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn len_and_is_empty_track_store() {
        let snap = MempoolSnapshot::default();
        assert_eq!(snap.len(), 0);
        assert!(snap.is_empty());

        snap.store(HashSet::from(["a".to_string(), "b".to_string()]));
        assert_eq!(snap.len(), 2);
        assert!(!snap.is_empty());

        snap.store(HashSet::new());
        assert_eq!(snap.len(), 0);
        assert!(snap.is_empty());
    }
}
