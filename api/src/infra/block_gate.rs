use shared::metrics::record_elapsed;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::OwnedMutexGuard;
use tokio::time::Instant;

/// Bounds the failure mode this gate introduces: a holder that never drops
/// (a hung RPC, say) would otherwise stall `MempoolReconciler::tick`
/// indefinitely instead of just the block side that got stuck.
pub const MAX_HOLD: Duration = Duration::from_secs(300);

/// Counts a gate that reopened itself past `MAX_HOLD` while its holder was
/// still alive, i.e. the ceiling actually firing rather than a normal release.
const BLOCK_GATE_EXPIRED_TOTAL: &str = "block_gate_expired_total";

/// How long `acquire` waited for an in-flight tick to finish.
const BLOCK_GATE_WAIT_SECONDS: &str = "block_gate_wait_seconds";

/// Only a warning, never a ceiling: giving up on the wait would put the block's
/// writes back alongside the tick's, which is what the wait exists to prevent.
const SLOW_WAIT: Duration = Duration::from_secs(60);

/// Coordinates `apply_block` with `MempoolReconciler::tick` so a flush never
/// lands while a block is being applied and races `confirmed_at`.
///
/// Two halves. `held_since` keeps new ticks out while a block is applied.
/// `in_flight` is held by a tick for its whole run, so `acquire` can wait out
/// the one that had already started when the block arrived, and never writes
/// alongside it.
///
/// Single holder only. `apply_block` is never concurrent with itself, so this
/// does not count holders or queue waiters -- a second concurrent `acquire`
/// just clobbers the first. Add that only if the calling contract changes.
#[derive(Clone, Default)]
pub struct BlockGate {
    held_since: Arc<Mutex<Option<Instant>>>,
    in_flight: Arc<tokio::sync::Mutex<()>>,
}

/// Reopens the gate when dropped, so an early `return` or an error path in
/// the holder still releases it.
#[must_use = "the gate is held only as long as this guard is alive"]
pub struct BlockGateGuard {
    held_since: Arc<Mutex<Option<Instant>>>,
}

/// A tick's claim on the gate, released on drop.
#[must_use = "the tick is in flight only as long as this lease is alive"]
pub struct FlushLease {
    _in_flight: OwnedMutexGuard<()>,
}

impl BlockGate {
    /// Shuts the gate, then waits for any tick already in flight to finish.
    ///
    /// Never waits on a previous holder -- see the single-holder note on the
    /// type.
    pub async fn acquire(&self) -> BlockGateGuard {
        *self.held_since.lock().expect("block gate poisoned") = Some(Instant::now());
        // Built before the wait, so a cancelled `acquire` still reopens the gate.
        let guard = BlockGateGuard {
            held_since: Arc::clone(&self.held_since),
        };

        let waiting_since = std::time::Instant::now();
        let in_flight = self.in_flight.lock();
        tokio::pin!(in_flight);
        if tokio::time::timeout(SLOW_WAIT, &mut in_flight)
            .await
            .is_err()
        {
            tracing::warn!("block gate: still waiting for an in-flight tick after {SLOW_WAIT:?}");
            drop(in_flight.await);
        }
        record_elapsed(BLOCK_GATE_WAIT_SECONDS, &[], waiting_since);

        guard
    }

    /// Claims the gate for one tick, or `None` while a block is being applied.
    pub fn try_begin_flush(&self) -> Option<FlushLease> {
        let lease = Arc::clone(&self.in_flight).try_lock_owned().ok()?;
        // Checked only once the lease is ours: an `acquire` landing after this
        // sees the lease and waits for the tick instead of racing it.
        if self.is_held() {
            return None;
        }
        Some(FlushLease { _in_flight: lease })
    }

    /// Whether a holder currently has the gate shut.
    ///
    /// Non-blocking, but may reopen the gate if the holder has exceeded `MAX_HOLD`.
    pub fn is_held(&self) -> bool {
        let mut held_since = self.held_since.lock().expect("block gate poisoned");
        match *held_since {
            Some(acquired_at) if acquired_at.elapsed() < MAX_HOLD => true,
            Some(_) => {
                tracing::warn!(
                    "block gate held past its {MAX_HOLD:?} ceiling, reopening while the holder is still alive"
                );
                metrics::counter!(BLOCK_GATE_EXPIRED_TOTAL).increment(1);
                *held_since = None;
                false
            }
            None => false,
        }
    }
}

impl Drop for BlockGateGuard {
    fn drop(&mut self) {
        *self.held_since.lock().expect("block gate poisoned") = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testkit::metrics::{assert_series, capture};

    #[tokio::test]
    async fn a_fresh_gate_is_open() {
        let gate = BlockGate::default();
        assert!(!gate.is_held());
        assert!(gate.try_begin_flush().is_some());
    }

    #[tokio::test]
    async fn the_gate_is_held_while_a_guard_lives() {
        let gate = BlockGate::default();
        let _guard = gate.acquire().await;
        assert!(gate.is_held());
        assert!(gate.try_begin_flush().is_none());
    }

    #[tokio::test]
    async fn the_gate_reopens_when_the_guard_drops() {
        let gate = BlockGate::default();
        let guard = gate.acquire().await;
        assert!(gate.is_held());

        drop(guard);
        assert!(!gate.is_held());
        assert!(gate.try_begin_flush().is_some());
    }

    #[tokio::test]
    async fn only_one_tick_is_in_flight_at_a_time() {
        let gate = BlockGate::default();
        let _lease = gate.try_begin_flush().expect("open gate");
        assert!(gate.try_begin_flush().is_none());
    }

    #[tokio::test]
    async fn acquire_waits_for_the_tick_already_in_flight() {
        let gate = BlockGate::default();
        let lease = gate.try_begin_flush().expect("open gate");

        let acquiring = tokio::spawn({
            let gate = gate.clone();
            async move { gate.acquire().await }
        });

        // Shut as soon as the block arrives, even before the tick is done.
        while !gate.is_held() {
            tokio::task::yield_now().await;
        }
        assert!(
            !acquiring.is_finished(),
            "must not return while the tick still holds its lease"
        );

        drop(lease);
        let _guard = acquiring.await.expect("acquire panicked");
        assert!(gate.is_held());
    }

    #[tokio::test]
    async fn a_tick_cannot_begin_while_acquire_is_waiting() {
        let gate = BlockGate::default();
        let lease = gate.try_begin_flush().expect("open gate");

        let acquiring = tokio::spawn({
            let gate = gate.clone();
            async move { gate.acquire().await }
        });
        while !gate.is_held() {
            tokio::task::yield_now().await;
        }

        drop(lease);
        assert!(
            gate.try_begin_flush().is_none(),
            "the block is waiting its turn; a new tick must not jump ahead of it"
        );
        let _guard = acquiring.await.expect("acquire panicked");
    }

    #[tokio::test(start_paused = true)]
    async fn the_gate_reports_open_once_the_ceiling_elapses_even_though_the_guard_still_lives() {
        let gate = BlockGate::default();
        let _guard = gate.acquire().await;

        tokio::time::advance(MAX_HOLD + Duration::from_secs(1)).await;

        assert!(
            !gate.is_held(),
            "the ceiling must win even though the guard was never dropped"
        );
        assert!(
            gate.try_begin_flush().is_some(),
            "the guard does not hold the lease, so a tick can run again"
        );
    }

    #[test]
    fn a_holder_that_outlives_the_ceiling_is_counted() {
        let rendered = capture(async {
            tokio::time::pause();
            let gate = BlockGate::default();
            let _guard = gate.acquire().await;

            tokio::time::advance(MAX_HOLD + Duration::from_secs(1)).await;
            assert!(!gate.is_held());
        });

        assert_series(&rendered, "block_gate_expired_total 1");
    }

    #[test]
    fn every_acquire_records_its_wait() {
        let rendered = capture(async {
            let gate = BlockGate::default();
            drop(gate.acquire().await);
            drop(gate.acquire().await);
        });

        assert_series(&rendered, "block_gate_wait_seconds_count 2");
    }
}
