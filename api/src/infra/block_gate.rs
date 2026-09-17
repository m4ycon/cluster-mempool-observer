use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

/// Bounds the failure mode this gate introduces: a holder that never drops
/// (a hung RPC, say) would otherwise stall `MempoolReconciler::tick`
/// indefinitely instead of just the block side that got stuck.
pub const MAX_HOLD: Duration = Duration::from_secs(300);

/// Counts a gate that reopened itself past `MAX_HOLD` while its holder was
/// still alive, i.e. the ceiling actually firing rather than a normal release.
const BLOCK_GATE_EXPIRED_TOTAL: &str = "block_gate_expired_total";

/// Coordinates `apply_block` with `MempoolReconciler::tick` so a flush never
/// lands while a block is being applied and races `confirmed_at`.
///
/// Single holder only. `apply_block` is never concurrent with itself, so this
/// does not count holders or queue waiters -- a second concurrent `acquire`
/// just clobbers the first. Add that only if the calling contract changes.
#[derive(Clone, Default)]
pub struct BlockGate {
    held_since: Arc<Mutex<Option<Instant>>>,
}

/// Reopens the gate when dropped, so an early `return` or an error path in
/// the holder still releases it.
#[must_use = "the gate is held only as long as this guard is alive"]
pub struct BlockGateGuard {
    held_since: Arc<Mutex<Option<Instant>>>,
}

impl BlockGate {
    /// Shuts the gate and returns the guard that reopens it on drop.
    ///
    /// Never blocks or waits on a previous holder -- see the single-holder
    /// note on the type.
    pub fn acquire(&self) -> BlockGateGuard {
        *self.held_since.lock().expect("block gate poisoned") = Some(Instant::now());
        BlockGateGuard {
            held_since: Arc::clone(&self.held_since),
        }
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
    }

    #[tokio::test]
    async fn the_gate_is_held_while_a_guard_lives() {
        let gate = BlockGate::default();
        let _guard = gate.acquire();
        assert!(gate.is_held());
    }

    #[tokio::test]
    async fn the_gate_reopens_when_the_guard_drops() {
        let gate = BlockGate::default();
        let guard = gate.acquire();
        assert!(gate.is_held());

        drop(guard);
        assert!(!gate.is_held());
    }

    #[tokio::test(start_paused = true)]
    async fn the_gate_reports_open_once_the_ceiling_elapses_even_though_the_guard_still_lives() {
        let gate = BlockGate::default();
        let _guard = gate.acquire();

        tokio::time::advance(MAX_HOLD + Duration::from_secs(1)).await;

        assert!(
            !gate.is_held(),
            "the ceiling must win even though the guard was never dropped"
        );
    }

    #[test]
    fn a_holder_that_outlives_the_ceiling_is_counted() {
        let rendered = capture(async {
            tokio::time::pause();
            let gate = BlockGate::default();
            let _guard = gate.acquire();

            tokio::time::advance(MAX_HOLD + Duration::from_secs(1)).await;
            assert!(!gate.is_held());
        });

        assert_series(&rendered, "block_gate_expired_total 1");
    }
}
