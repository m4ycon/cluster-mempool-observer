use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// Live, shareable snapshot of the mempool-delta watcher's tracked txid set.
#[derive(Clone, Default)]
pub struct MempoolSnapshot {
    inner: Arc<RwLock<HashSet<String>>>,
}

impl MempoolSnapshot {
    // TODO: maybe that's a bit heavy if we are storing a lot of txids (>10k),
    // although we would have it in memory anyway (rpc call), so maybe it's fine.
    pub fn store(&self, txids: HashSet<String>) {
        *self.inner.write().expect("mempool snapshot poisoned") = txids;
    }

    pub fn get(&self) -> HashSet<String> {
        self.inner
            .read()
            .expect("mempool snapshot poisoned")
            .clone()
    }

    pub fn contains(&self, txid: &str) -> bool {
        self.inner
            .read()
            .expect("mempool snapshot poisoned")
            .contains(txid)
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

    #[test]
    fn contains_tracks_store() {
        let snap = MempoolSnapshot::default();
        assert!(!snap.contains("a"));

        snap.store(HashSet::from(["a".to_string()]));
        assert!(snap.contains("a"));
        assert!(!snap.contains("b"));

        snap.store(HashSet::new());
        assert!(!snap.contains("a"));
    }
}
