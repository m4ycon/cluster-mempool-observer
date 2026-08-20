use crate::clients::rpc_client::RpcClient;
use crate::error::ObserverError;
use crate::watchers::watcher_trait::{Watcher, WatcherRPC};
use shared::api::{FeerateDiagramPoint, MempoolFeerateDiagram};
use shared::models::GetMempoolFeerateDiagramRaw;
use shared::snapshot::FeerateDiagramSnapshot;
use shared::subjects::Subject;
use time::OffsetDateTime;

pub struct FeerateDiagramWatcher {
    rpc: RpcClient,
    watch_rate: u32,
    tracker: FeerateDiagramTracker,
    snapshot: FeerateDiagramSnapshot,
}

impl FeerateDiagramWatcher {
    pub fn new(rpc: RpcClient, watch_rate: u32, snapshot: FeerateDiagramSnapshot) -> Self {
        Self {
            rpc,
            watch_rate,
            tracker: FeerateDiagramTracker::default(),
            snapshot,
        }
    }

    /// Stores the fresh points and reports whether they changed. Storing
    /// always happens, even unchanged, so `sampled_at` keeps advancing.
    fn record(&mut self, points: Vec<FeerateDiagramPoint>) -> bool {
        let changed = self.tracker.update(&points);
        self.snapshot.store(MempoolFeerateDiagram {
            sampled_at: Some(OffsetDateTime::now_utc()),
            points,
        });
        changed
    }
}

impl Watcher for FeerateDiagramWatcher {
    type Event = MempoolFeerateDiagram;

    fn get_publish_subject(&self) -> Subject {
        Subject::MempoolFeerateDiagram
    }
}

impl WatcherRPC for FeerateDiagramWatcher {
    type Response = ();

    async fn watch(&mut self) -> Result<Option<Self::Response>, ObserverError> {
        let raw: GetMempoolFeerateDiagramRaw = self
            .rpc
            .call("getmempoolfeeratediagram", |client| {
                // TODO: change this raw call when new release of corepc is updated
                // (current 0.15), which has no binding for this RPC at all
                client.call("getmempoolfeeratediagram", &[])
            })
            .await?;

        let points = Vec::<FeerateDiagramPoint>::try_from(&raw)
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))?;

        Ok(self.record(points).then_some(()))
    }

    fn to_event(&self, _response: &Self::Response) -> Self::Event {
        self.snapshot.get()
    }

    fn get_watch_rate(&self) -> u32 {
        self.watch_rate
    }
}

/// Tracks the last-seen feerate diagram points for change detection.
#[derive(Default)]
struct FeerateDiagramTracker {
    last_points: Vec<FeerateDiagramPoint>,
}

impl FeerateDiagramTracker {
    /// Replaces the tracked points if they differ. Returns true if they changed.
    fn update(&mut self, points: &[FeerateDiagramPoint]) -> bool {
        if self.last_points == points {
            return false;
        }
        self.last_points = points.to_vec();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::config::RpcConfig;

    fn point(weight: u64, fee_sats: i64) -> FeerateDiagramPoint {
        FeerateDiagramPoint { weight, fee_sats }
    }

    fn dummy_rpc() -> RpcClient {
        RpcClient::new(&RpcConfig {
            host: "127.0.0.1:18443".into(),
            user: "user".into(),
            pass: "pass".into(),
        })
        .expect("build rpc client")
    }

    fn watcher() -> FeerateDiagramWatcher {
        FeerateDiagramWatcher::new(dummy_rpc(), 1, FeerateDiagramSnapshot::default())
    }

    #[test]
    fn first_poll_reports_a_change() {
        let mut w = watcher();

        assert!(w.record(vec![point(441, 2712)]));
    }

    #[test]
    fn unchanged_points_between_polls_report_no_change() {
        let mut w = watcher();
        w.record(vec![point(441, 2712)]);

        assert!(!w.record(vec![point(441, 2712)]));
    }

    #[test]
    fn changed_points_between_polls_report_a_change() {
        let mut w = watcher();
        w.record(vec![point(441, 2712)]);

        assert!(w.record(vec![point(441, 5000)]));
    }

    #[test]
    fn snapshot_is_written_and_sampled_at_advances_even_when_unchanged() {
        let mut w = watcher();
        w.record(vec![point(441, 2712)]);
        let first_sampled_at = w.snapshot.get().sampled_at.expect("sampled_at set");

        std::thread::sleep(std::time::Duration::from_millis(2));
        let changed = w.record(vec![point(441, 2712)]);

        assert!(!changed);
        let second = w.snapshot.get();
        assert_eq!(second.points.len(), 1);
        assert_eq!(second.points[0].weight, 441);
        assert_eq!(second.points[0].fee_sats, 2712);
        let second_sampled_at = second.sampled_at.expect("sampled_at set");
        assert!(
            second_sampled_at > first_sampled_at,
            "sampled_at should advance on every poll, even without a change"
        );
    }
}
