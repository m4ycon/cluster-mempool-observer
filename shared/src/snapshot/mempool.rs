use crate::models::DeltaDirection;
use std::collections::{HashMap, HashSet, VecDeque};
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

/// Cap on non-persisted/unflushed journal entries.
const JOURNAL_CAP: usize = 250_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub txid: String,
    pub direction: DeltaDirection,
}

/// Single owner of "what is in our mempool right now".
#[derive(Clone, Default)]
pub struct MempoolLedger {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Default)]
struct Inner {
    /// The set of txids currently in the mempool.
    live: HashSet<String>,
    /// In/out history of the mempool.
    journal: VecDeque<JournalEntry>,
    /// Whether the journal has ever overflowed and lost entries.
    degraded: bool,
}

impl Inner {
    /// Queue a new journal entry and return whether it was accepted (can be dropped
    /// if the journal is full, although reconcile_against will eventually catch up).
    fn push(&mut self, txid: String, direction: DeltaDirection) -> bool {
        if self.journal.len() >= JOURNAL_CAP {
            self.degraded = true;
            return false;
        }
        self.journal.push_back(JournalEntry { txid, direction });
        true
    }
}

/// A prefix of the journal read by `begin_flush`.
pub struct FlushBatch {
    inner: Arc<RwLock<Inner>>,
    entries: Vec<JournalEntry>,
}

impl FlushBatch {
    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    /// Consumes the batch so it cannot be committed twice.
    pub fn commit(self) {
        let mut inner = self.inner.write().expect("mempool ledger poisoned");
        // Only the consumer removes from the journal, and only from the
        // front; generators only ever append to the back. That invariant is
        // what makes "the first N" the same prefix here as it was at
        // `begin_flush`.
        inner.journal.drain(..self.entries.len());
    }

    /// Transitions per direction, as `(added, removed)`.
    pub fn transition_counts(&self) -> (usize, usize) {
        self.entries
            .iter()
            .fold((0, 0), |(added, removed), entry| match entry.direction {
                DeltaDirection::Add => (added + 1, removed),
                DeltaDirection::Remove => (added, removed + 1),
            })
    }

    pub fn added_txids(&self) -> Vec<String> {
        distinct_txids(&self.entries, DeltaDirection::Add)
    }

    pub fn residency_delta(&self) -> (Vec<String>, Vec<String>) {
        let mut first_last: HashMap<&str, (DeltaDirection, DeltaDirection)> = HashMap::new();
        for entry in &self.entries {
            first_last
                .entry(entry.txid.as_str())
                .and_modify(|(_, last)| *last = entry.direction)
                .or_insert((entry.direction, entry.direction));
        }

        let mut added = Vec::new();
        let mut removed = Vec::new();
        for (txid, (first, last)) in first_last {
            match (first, last) {
                (DeltaDirection::Add, DeltaDirection::Add) => added.push(txid.to_string()),
                (DeltaDirection::Remove, DeltaDirection::Remove) => removed.push(txid.to_string()),
                _ => {}
            }
        }
        (added, removed)
    }
}

impl MempoolLedger {
    /// Replaces `live` with `txids` and touches nothing else: no journal entry.
    pub fn seed(&self, txids: HashSet<String>) {
        self.inner.write().expect("mempool ledger poisoned").live = txids;
    }

    /// Diffs the full `getrawmempool` result against `live` and swaps it in.
    pub fn submit_authoritative(&self, txids: HashSet<String>) {
        let mut inner = self.inner.write().expect("mempool ledger poisoned");

        let to_add: Vec<String> = txids
            .iter()
            .filter(|txid| !inner.live.contains(txid.as_str()))
            .cloned()
            .collect();
        let to_remove: Vec<String> = inner.live.difference(&txids).cloned().collect();

        for txid in to_add {
            inner.push(txid, DeltaDirection::Add);
        }
        for txid in to_remove {
            inner.push(txid, DeltaDirection::Remove);
        }
        inner.live = txids;
    }

    /// Marks txids as present. Only txids `live` did not already hold produce an `Add`.
    pub fn assert_present(&self, txids: &[String]) {
        let mut inner = self.inner.write().expect("mempool ledger poisoned");
        for txid in txids {
            if inner.live.insert(txid.clone()) {
                inner.push(txid.clone(), DeltaDirection::Add);
            }
        }
    }

    /// Marks txids as absent. Only txids `live` actually held produce a `Remove`.
    pub fn assert_absent(&self, txids: &[String]) {
        let mut inner = self.inner.write().expect("mempool ledger poisoned");
        for txid in txids {
            if inner.live.remove(txid) {
                inner.push(txid.clone(), DeltaDirection::Remove);
            }
        }
    }

    /// Returns a batch of every entry currently in the journal.
    ///
    /// `None` if the journal is empty. Otherwise the whole journal, in order,
    /// without shrinking it -- only `commit` does that.
    pub fn begin_flush(&self) -> Option<FlushBatch> {
        let inner = self.inner.read().expect("mempool ledger poisoned");
        if inner.journal.is_empty() {
            return None;
        }
        let entries = inner.journal.iter().cloned().collect();
        drop(inner);
        Some(FlushBatch {
            inner: Arc::clone(&self.inner),
            entries,
        })
    }

    pub fn contains(&self, txid: &str) -> bool {
        self.inner
            .read()
            .expect("mempool ledger poisoned")
            .live
            .contains(txid)
    }

    pub fn len(&self) -> usize {
        self.inner
            .read()
            .expect("mempool ledger poisoned")
            .live
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Lets the caller compute over `live` under the read lock instead of
    /// cloning the whole set.
    pub fn with_live<R>(&self, f: impl FnOnce(&HashSet<String>) -> R) -> R {
        // TODO: maybe remove this method, as it doesn't follow the same pattern as the other methods (e.g. contains)
        f(&self.inner.read().expect("mempool ledger poisoned").live)
    }

    pub fn clone_live_snapshot(&self) -> HashSet<String> {
        self.inner
            .read()
            .expect("mempool ledger poisoned")
            .live
            .clone()
    }

    pub fn pending_len(&self) -> usize {
        self.inner
            .read()
            .expect("mempool ledger poisoned")
            .journal
            .len()
    }

    pub fn is_degraded(&self) -> bool {
        self.inner.read().expect("mempool ledger poisoned").degraded
    }

    /// Repairs the journal after the database was unreachable. `db_image` is
    /// the residency the database was reconstructed to hold.
    pub fn reconcile_against(&self, db_image: HashSet<String>) {
        let mut inner = self.inner.write().expect("mempool ledger poisoned");
        let mut eventual = db_image;

        // Folding the still-pending journal onto `db_image` gives the residency
        // the database will eventually hold once that journal drains, not the
        // residency it holds right now.
        for entry in &inner.journal {
            match entry.direction {
                DeltaDirection::Add => {
                    eventual.insert(entry.txid.clone());
                }
                DeltaDirection::Remove => {
                    eventual.remove(&entry.txid);
                }
            }
        }

        // Diffing against `eventual`, not `db_image`, is what stops this from
        // re-queuing an `Add` for a txid whose `Add` is already in the
        // journal -- i.e. what stops the repair from reintroducing the
        // duplicate-event bug this ledger exists to kill.
        let to_add: Vec<String> = inner.live.difference(&eventual).cloned().collect();
        let to_remove: Vec<String> = eventual.difference(&inner.live).cloned().collect();

        // means "did everything that I needed to be queued actually fit in the journal"
        let mut complete = true;
        for txid in to_add {
            complete &= inner.push(txid, DeltaDirection::Add);
        }
        for txid in to_remove {
            complete &= inner.push(txid, DeltaDirection::Remove);
        }

        // A repair that didn't fully fit leaves the gap open; keeping `degraded`
        // set is what makes the next reconcile happen instead of the divergence
        // going permanently silent.
        if complete {
            inner.degraded = false;
        }
    }
}

/// The distinct txids among `entries` moving in `direction`.
pub fn distinct_txids(entries: &[JournalEntry], direction: DeltaDirection) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| entry.direction == direction)
        .map(|entry| entry.txid.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
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

    fn txids(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| s.to_string()).collect()
    }

    fn set(slice: &[&str]) -> HashSet<String> {
        slice.iter().map(|s| s.to_string()).collect()
    }

    fn directions(batch: &FlushBatch) -> Vec<(String, DeltaDirection)> {
        batch
            .entries()
            .iter()
            .map(|e| (e.txid.clone(), e.direction))
            .collect()
    }

    fn expect(pairs: &[(&str, DeltaDirection)]) -> Vec<(String, DeltaDirection)> {
        pairs.iter().map(|(t, d)| (t.to_string(), *d)).collect()
    }

    #[test]
    fn seed_fills_live_and_queues_nothing() {
        let ledger = MempoolLedger::default();
        ledger.seed(set(&["a", "b"]));

        assert_eq!(ledger.clone_live_snapshot(), set(&["a", "b"]));
        assert_eq!(ledger.pending_len(), 0);
    }

    #[test]
    fn submit_authoritative_on_empty_ledger_adds_every_txid() {
        let ledger = MempoolLedger::default();
        ledger.submit_authoritative(set(&["a", "b"]));

        let batch = ledger.begin_flush().expect("entries queued");
        let mut seen = directions(&batch);
        seen.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            seen,
            expect(&[("a", DeltaDirection::Add), ("b", DeltaDirection::Add)])
        );
    }

    #[test]
    fn submit_authoritative_with_same_set_twice_queues_nothing_the_second_time() {
        let ledger = MempoolLedger::default();
        ledger.submit_authoritative(set(&["a", "b"]));
        ledger.begin_flush().expect("entries queued").commit();

        ledger.submit_authoritative(set(&["a", "b"]));
        assert!(ledger.begin_flush().is_none());
    }

    #[test]
    fn submit_authoritative_dropping_a_present_txid_queues_remove() {
        let ledger = MempoolLedger::default();
        ledger.submit_authoritative(set(&["a", "b"]));
        ledger.begin_flush().expect("entries queued").commit();

        ledger.submit_authoritative(set(&["a"]));
        let batch = ledger.begin_flush().expect("entries queued");
        assert_eq!(directions(&batch), expect(&[("b", DeltaDirection::Remove)]));
    }

    #[test]
    fn assert_present_of_an_already_authoritative_txid_queues_nothing() {
        let ledger = MempoolLedger::default();
        ledger.submit_authoritative(set(&["a"]));
        ledger.begin_flush().expect("entries queued").commit();

        ledger.assert_present(&txids(&["a"]));
        assert!(ledger.begin_flush().is_none());
    }

    #[test]
    fn assert_present_then_submit_authoritative_does_not_double_the_add() {
        // Reproduces the production bug at the ledger level: the cluster sync
        // (assert_present) and the delta watcher's getrawmempool diff
        // (submit_authoritative) both learn about the same txid, and it must
        // still produce exactly one add_mempool transition.
        let ledger = MempoolLedger::default();
        ledger.assert_present(&txids(&["x"]));
        ledger.submit_authoritative(set(&["x"]));

        let batch = ledger.begin_flush().expect("entries queued");
        assert_eq!(
            directions(&batch),
            expect(&[("x", DeltaDirection::Add)]),
            "x must be added exactly once even though both sources saw it"
        );
    }

    #[test]
    fn assert_present_then_empty_authoritative_preserves_add_then_remove_churn() {
        let ledger = MempoolLedger::default();
        ledger.assert_present(&txids(&["x"]));
        ledger.submit_authoritative(HashSet::new());

        let batch = ledger.begin_flush().expect("entries queued");
        assert_eq!(
            directions(&batch),
            expect(&[("x", DeltaDirection::Add), ("x", DeltaDirection::Remove)]),
            "this add/remove pair is what closes out a cluster for a txid that \
             entered and left between two flushes; collapsing it would leave \
             that cluster active forever"
        );
    }

    #[test]
    fn reentry_after_present_absent_present_yields_add_remove_add_in_order() {
        let ledger = MempoolLedger::default();
        ledger.assert_present(&txids(&["x"]));
        ledger.submit_authoritative(HashSet::new());
        ledger.assert_present(&txids(&["x"]));

        let batch = ledger.begin_flush().expect("entries queued");
        assert_eq!(
            directions(&batch),
            expect(&[
                ("x", DeltaDirection::Add),
                ("x", DeltaDirection::Remove),
                ("x", DeltaDirection::Add),
            ])
        );
    }

    #[test]
    fn begin_flush_does_not_shrink_the_journal_until_commit() {
        let ledger = MempoolLedger::default();
        ledger.submit_authoritative(set(&["a", "b"]));

        let batch = ledger.begin_flush().expect("entries queued");
        assert_eq!(ledger.pending_len(), 2);
        drop(batch);
        assert_eq!(ledger.pending_len(), 2);
    }

    #[test]
    fn dropping_a_batch_without_commit_replays_the_same_prefix_next_time() {
        let ledger = MempoolLedger::default();
        ledger.submit_authoritative(set(&["a", "b"]));

        let first = ledger.begin_flush().expect("entries queued");
        let first_seen = directions(&first);
        drop(first);

        let second = ledger.begin_flush().expect("entries queued");
        assert_eq!(directions(&second), first_seen);
    }

    #[test]
    fn a_batch_counts_transitions_but_reports_txids_deduped() {
        let ledger = MempoolLedger::default();
        // "a" leaves and comes back, so it costs three add entries between the
        // two txids while still being one txid to add.
        ledger.assert_present(&txids(&["a", "b"]));
        ledger.assert_absent(&txids(&["a"]));
        ledger.assert_present(&txids(&["a"]));

        let batch = ledger.begin_flush().expect("entries queued");
        assert_eq!(batch.transition_counts(), (3, 1));

        let mut added = batch.added_txids();
        added.sort();
        assert_eq!(added, txids(&["a", "b"]));
    }

    #[test]
    fn residency_delta_nets_out_churn_within_one_batch() {
        let ledger = MempoolLedger::default();
        // Seed the txids that need to already be resident going into the batch under test.
        ledger.submit_authoritative(set(&["rr", "ra"]));
        ledger.begin_flush().unwrap().commit();

        ledger.assert_present(&txids(&["aa"])); // Add only
        ledger.assert_absent(&txids(&["rr"])); // Remove only
        ledger.assert_present(&txids(&["ar"])); // Add...
        ledger.assert_absent(&txids(&["ar"])); // ...then Remove: net no-op
        ledger.assert_absent(&txids(&["ra"])); // Remove...
        ledger.assert_present(&txids(&["ra"])); // ...then Add: net no-op

        let batch = ledger.begin_flush().expect("entries queued");
        let (mut added, mut removed) = batch.residency_delta();
        added.sort();
        removed.sort();
        assert_eq!(added, txids(&["aa"]));
        assert_eq!(removed, txids(&["rr"]));
    }

    #[test]
    fn commit_removes_exactly_the_read_prefix_and_keeps_later_pushes() {
        let ledger = MempoolLedger::default();
        ledger.assert_present(&txids(&["a"]));

        let batch = ledger.begin_flush().expect("entries queued");
        assert_eq!(batch.entries().len(), 1);

        // Pushed while the batch is in flight: must survive the commit below.
        // This is what proves the flush is non-destructive -- only the
        // consumer removes, and only from the front, so a generator appending
        // to the back mid-flight can never race the read prefix away.
        ledger.assert_present(&txids(&["b", "c"]));

        batch.commit();
        assert_eq!(ledger.pending_len(), 2); // "b" and "c", pushed after begin_flush

        let rest = ledger.begin_flush().expect("entries queued");
        let rest_txids: Vec<&str> = rest.entries().iter().map(|e| e.txid.as_str()).collect();
        assert_eq!(rest_txids, vec!["b", "c"]);
    }

    #[test]
    fn overflow_marks_degraded_and_preserves_already_queued_entries() {
        let ledger = MempoolLedger::default();
        let many: Vec<String> = (0..JOURNAL_CAP + 10).map(|i| format!("tx{i}")).collect();
        ledger.assert_present(&many);

        assert!(ledger.is_degraded());
        assert_eq!(ledger.pending_len(), JOURNAL_CAP);

        let batch = ledger.begin_flush().expect("entries queued");
        let seen: Vec<&str> = batch.entries()[..3]
            .iter()
            .map(|e| e.txid.as_str())
            .collect();
        assert_eq!(seen, vec!["tx0", "tx1", "tx2"]);
    }

    #[test]
    fn reconcile_against_does_not_requeue_already_pending_transitions() {
        let ledger = MempoolLedger::default();
        ledger.assert_present(&txids(&["a", "b"]));
        ledger.begin_flush().unwrap().commit(); // db now holds {a, b}

        ledger.assert_present(&txids(&["c"])); // pending: Add c
        ledger.assert_absent(&txids(&["a"])); // pending: Remove a
        // live = {b, c}; pending journal = [Add c, Remove a]

        // db still shows what was actually committed, {a, b}. Applying the
        // pending journal to that lands on {b, c}, which already matches live --
        // reconcile must not add a second Add c or Remove a for that.
        ledger.reconcile_against(set(&["a", "b"]));

        let batch = ledger.begin_flush().expect("entries queued");
        assert_eq!(
            directions(&batch),
            expect(&[("c", DeltaDirection::Add), ("a", DeltaDirection::Remove)]),
            "reconcile must not re-queue what is already pending"
        );
    }

    #[test]
    fn reconcile_against_queues_the_gap_left_by_an_overflow() {
        let ledger = MempoolLedger::default();
        let fillers: Vec<String> = (0..JOURNAL_CAP).map(|i| format!("tx{i}")).collect();
        ledger.assert_present(&fillers); // fills the journal exactly to the cap
        assert!(!ledger.is_degraded());

        ledger.assert_present(&txids(&["lost"])); // journal full: refused, but "lost" is live
        assert!(ledger.is_degraded());

        ledger.begin_flush().unwrap().commit(); // db now holds the fillers

        // db_image matches exactly what was just committed; "lost" never made
        // it into the journal at all because of the overflow above -- this is
        // the gap reconcile exists to close once the database is reachable.
        ledger.reconcile_against(fillers.into_iter().collect());

        let batch = ledger.begin_flush().expect("entries queued");
        assert_eq!(directions(&batch), expect(&[("lost", DeltaDirection::Add)]));
    }

    #[test]
    fn reconcile_against_clears_degraded() {
        let ledger = MempoolLedger::default();
        let many: Vec<String> = (0..JOURNAL_CAP + 10).map(|i| format!("tx{i}")).collect();
        ledger.assert_present(&many);
        assert!(ledger.is_degraded());

        // Free up room first: with the journal still full, the repair itself
        // would be refused (see the sibling test below), so this checks the
        // happy path where the repair actually fits.
        ledger.begin_flush().unwrap().commit();
        ledger.reconcile_against(many[..JOURNAL_CAP].iter().cloned().collect());
        assert!(!ledger.is_degraded());
    }

    #[test]
    fn reconcile_against_leaves_degraded_set_when_its_own_repair_does_not_fit() {
        let ledger = MempoolLedger::default();
        let fillers: Vec<String> = (0..JOURNAL_CAP).map(|i| format!("tx{i}")).collect();
        ledger.assert_present(&fillers); // fills the journal exactly to the cap
        ledger.assert_present(&txids(&["lost"])); // refused: journal already full
        assert!(ledger.is_degraded());

        // Journal is never flushed here, so reconcile's own push for "lost"
        // hits the same cap and is refused too.
        ledger.reconcile_against(fillers.into_iter().collect());

        assert!(
            ledger.is_degraded(),
            "the repair push for the lost txid was itself refused, so the gap \
             is still open and degraded must survive, not clear over it"
        );
    }
}
