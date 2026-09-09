use crate::db::models::{MempoolSnapshotRow, NewMempoolSnapshotRow};
use crate::db::{NATIVE_RESOLUTION_SECS, SnapshotRepository};
use crate::error::ApiError;
use shared::api::{MempoolMetricPoint, MempoolMetricSeries, SnapshotMetric};
use shared::snapshot::{ClusterSnapshot, MempoolLedger};
use std::time::Duration as StdDuration;
use time::{Duration, OffsetDateTime};
use tokio::time::MissedTickBehavior;

/// Bucket sizes the read path may choose, smallest (native) first.
const RESOLUTION_LADDER_SECS: [i64; 7] = [
    NATIVE_RESOLUTION_SECS, // 1 min
    5 * 60,
    10 * 60,
    30 * 60,
    60 * 60,      // 1 hour
    6 * 60 * 60,  // 6 hours
    24 * 60 * 60, // 1 day
];

/// Points a series may hold; caps how many buckets a range gets divided into.
const MAX_POINTS: i64 = 2000;

/// Default lookback for a range with no explicit `from`.
const DEFAULT_RANGE: Duration = Duration::hours(24);

/// Member txids of active clusters that are not in the live mempool set.
const SNAPSHOT_STALE_MEMBER_COUNT: &str = "cluster_stale_member_count";

#[derive(Clone)]
pub struct SnapshotService {
    snapshot_repository: SnapshotRepository,
    cluster_snapshot: ClusterSnapshot,
    mempool_ledger: MempoolLedger,
}

impl SnapshotService {
    pub fn new(
        snapshot_repository: SnapshotRepository,
        cluster_snapshot: ClusterSnapshot,
        mempool_ledger: MempoolLedger,
    ) -> Self {
        Self {
            snapshot_repository,
            cluster_snapshot,
            mempool_ledger,
        }
    }

    /// Samples forever, once per `interval`.
    pub async fn run(&self, interval: StdDuration) {
        let mut ticker = tokio::time::interval(interval);
        // Default `Burst` replays every missed tick back-to-back after a stall,
        // which would falsify the "one sample per minute" guarantee. A skipped
        // tick is an honest gap instead.
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            self.sample().await;
        }
    }

    /// Samples once, inserting a new row into the `mempool_snapshots` table.
    pub async fn sample(&self) {
        // Both numbers read `live` under the same lock acquisition, so the gauge
        // never compares a mempool snapshot to a cluster snapshot taken at a
        // different instant.
        let (stale_member_count, live_len) = self
            .mempool_ledger
            .with_live(|live| (self.cluster_snapshot.missing_member_count(live), live.len()));
        metrics::gauge!(SNAPSHOT_STALE_MEMBER_COUNT).set(stale_member_count as f64);

        let row = build_row(&self.cluster_snapshot, live_len, OffsetDateTime::now_utc());
        if let Err(e) = self.snapshot_repository.insert(&row).await {
            tracing::warn!("snapshot: failed to insert sample: {e}");
        }
    }

    /// Same range/resolution rules as `fetch_range`, projected down to one column --
    /// the table stays wide, only the response narrows.
    pub async fn metric_series(
        &self,
        metric: SnapshotMetric,
        from: Option<OffsetDateTime>,
        to: Option<OffsetDateTime>,
    ) -> Result<MempoolMetricSeries, ApiError> {
        let (resolution_secs, rows) = self.fetch_range(from, to).await?;
        Ok(MempoolMetricSeries {
            metric,
            resolution_secs,
            points: rows
                .into_iter()
                .map(|row| MempoolMetricPoint {
                    sampled_at: row.sampled_at,
                    value: metric_value(metric, &row),
                })
                .collect(),
        })
    }

    async fn fetch_range(
        &self,
        from: Option<OffsetDateTime>,
        to: Option<OffsetDateTime>,
    ) -> Result<(i64, Vec<MempoolSnapshotRow>), ApiError> {
        let to = to.unwrap_or_else(OffsetDateTime::now_utc);
        let from = from.unwrap_or(to - DEFAULT_RANGE);
        if from >= to {
            return Err(ApiError::BadRequest(
                "`from` must be earlier than `to`".to_string(),
            ));
        }

        let resolution_secs = pick_resolution(to - from);
        let rows = self
            .snapshot_repository
            .range(from, to, resolution_secs)
            .await?;
        Ok((resolution_secs, rows))
    }
}

fn metric_value(metric: SnapshotMetric, row: &MempoolSnapshotRow) -> i64 {
    match metric {
        SnapshotMetric::ClusterCount => row.cluster_count as i64,
        SnapshotMetric::ClusteredTxCount => row.clustered_tx_count as i64,
        SnapshotMetric::MempoolTxCount => row.mempool_tx_count as i64,
        SnapshotMetric::TotalVsize => row.total_vsize,
        SnapshotMetric::TotalFee => row.total_fee,
    }
}

/// Smallest ladder rung that keeps `span` within `MAX_POINTS` buckets, or the
/// widest rung if even that does not fit.
fn pick_resolution(span: Duration) -> i64 {
    let span_secs = span.whole_seconds();
    RESOLUTION_LADDER_SECS
        .into_iter()
        .find(|res| span_secs / res <= MAX_POINTS)
        .unwrap_or(*RESOLUTION_LADDER_SECS.last().expect("ladder is not empty"))
}

/// Builds one `mempool_snapshots` row from the current in-memory state.
fn build_row(
    cluster_snapshot: &ClusterSnapshot,
    mempool_tx_count: usize,
    sampled_at: OffsetDateTime,
) -> NewMempoolSnapshotRow {
    let stats = cluster_snapshot.stats();
    NewMempoolSnapshotRow {
        sampled_at,
        cluster_count: stats.cluster_count as i32,
        clustered_tx_count: stats.tx_count as i32,
        mempool_tx_count: mempool_tx_count as i32,
        total_vsize: stats.total_vsize,
        total_fee: stats.total_fee,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::events::ClusterRef;
    use testkit::fixtures::fixed_time;

    fn cluster_ref(id: i64, txids: &[&str], total_vsize: i64, total_fee: i64) -> ClusterRef {
        ClusterRef {
            id,
            txids: txids.iter().map(|s| s.to_string()).collect(),
            total_vsize,
            total_fee,
            first_seen_at: fixed_time(),
        }
    }

    #[test]
    fn build_row_carries_snapshot_values() {
        let cluster_snapshot = ClusterSnapshot::default();
        cluster_snapshot.upsert(cluster_ref(1, &["a", "b"], 200, 900));
        cluster_snapshot.upsert(cluster_ref(2, &["c"], 100, 300));

        let mempool_ledger = MempoolLedger::default();
        mempool_ledger.seed(["a", "b", "c", "d"].into_iter().map(String::from).collect());

        let sampled_at = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let row = build_row(&cluster_snapshot, mempool_ledger.len(), sampled_at);

        assert_eq!(row.sampled_at, sampled_at);
        assert_eq!(row.cluster_count, 2);
        assert_eq!(row.clustered_tx_count, 3); // a, b, c across both clusters
        assert_eq!(row.mempool_tx_count, 4); // a, b, c, d
        assert_eq!(row.total_vsize, 300);
        assert_eq!(row.total_fee, 1200);
    }

    #[tokio::test]
    async fn sample_does_not_panic_when_insert_fails() {
        let repo = SnapshotRepository::new(testkit::postgres::inert_pool());
        let cluster_snapshot = ClusterSnapshot::default();
        let mempool_ledger = MempoolLedger::default();

        // the inert pool never connects, so a direct insert must fail...
        let row = build_row(
            &cluster_snapshot,
            mempool_ledger.len(),
            OffsetDateTime::now_utc(),
        );
        assert!(repo.insert(&row).await.is_err());

        // ...but sample() must swallow that error, not panic.
        let service = SnapshotService::new(repo, cluster_snapshot, mempool_ledger);
        service.sample().await;
    }

    fn inert_service() -> SnapshotService {
        SnapshotService::new(
            SnapshotRepository::new(testkit::postgres::inert_pool()),
            ClusterSnapshot::default(),
            MempoolLedger::default(),
        )
    }

    #[test]
    fn pick_resolution_uses_native_cadence_for_a_day() {
        assert_eq!(pick_resolution(Duration::hours(24)), 60);
    }

    #[test]
    fn pick_resolution_steps_up_the_ladder_as_the_range_widens() {
        assert_eq!(pick_resolution(Duration::days(3)), 300);
        assert_eq!(pick_resolution(Duration::days(30)), 1_800);
        assert_eq!(pick_resolution(Duration::days(200)), 21_600);
    }

    #[test]
    fn pick_resolution_keeps_points_within_the_cap_when_a_rung_fits() {
        let span = Duration::days(200);
        let points = span.whole_seconds() / pick_resolution(span);
        assert!(points <= MAX_POINTS);
    }

    #[test]
    fn pick_resolution_clamps_to_the_widest_rung_for_an_extreme_range() {
        // 10 years: even the widest (daily) bucket exceeds MAX_POINTS, so the
        // ladder just stops climbing instead of inventing a coarser one.
        assert_eq!(pick_resolution(Duration::days(3_650)), 86_400);
    }

    #[tokio::test]
    async fn metric_series_surfaces_a_repository_failure_instead_of_an_empty_series() {
        let service = inert_service();
        assert!(matches!(
            service
                .metric_series(SnapshotMetric::ClusterCount, None, None)
                .await,
            Err(ApiError::Internal(_))
        ));
    }

    #[tokio::test]
    async fn fetch_range_rejects_a_from_not_earlier_than_to() {
        let service = inert_service();
        let to = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        assert!(matches!(
            service.fetch_range(Some(to), Some(to)).await,
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn metric_value_maps_each_variant_to_its_own_column() {
        let row = MempoolSnapshotRow {
            sampled_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
            cluster_count: 11,
            clustered_tx_count: 22,
            mempool_tx_count: 33,
            total_vsize: 44,
            total_fee: 55,
        };

        assert_eq!(metric_value(SnapshotMetric::ClusterCount, &row), 11);
        assert_eq!(metric_value(SnapshotMetric::ClusteredTxCount, &row), 22);
        assert_eq!(metric_value(SnapshotMetric::MempoolTxCount, &row), 33);
        assert_eq!(metric_value(SnapshotMetric::TotalVsize, &row), 44);
        assert_eq!(metric_value(SnapshotMetric::TotalFee, &row), 55);
    }
}
