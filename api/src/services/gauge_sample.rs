use crate::db::models::{MempoolGaugeSampleRow, NewMempoolGaugeSampleRow};
use crate::db::{ClusterRepository, GaugeSampleRepository};
use crate::error::ApiError;
use crate::services::resolution::{DEFAULT_RANGE, pick_resolution};
use shared::api::{FeerateDiagramComparison, GaugeMetric, GaugePoint, GaugeSeries};
use shared::feerate_comparison::{DiagramComparison, DiagramSegment, FeerateDiagramDiffUnit};
use shared::models::vsize_from_weight;
use shared::snapshot::cluster::ClusterStats;
use shared::snapshot::{
    ClusterSnapshot, FeerateComparisonSnapshot, FeerateDiagramSnapshot, MempoolLedger,
};
use std::time::Duration as StdDuration;
use time::{Duration, OffsetDateTime};
use tokio::time::MissedTickBehavior;

/// Member txids of active clusters that are not in the live mempool set.
const GAUGE_STALE_MEMBER_COUNT: &str = "cluster_stale_member_count";

const FEERATE_DIAGRAM_MIN_AGE: Duration = Duration::seconds(15);
const FEERATE_DIAGRAM_MAX_AGE: Duration = Duration::seconds(60);

#[derive(Clone)]
pub struct GaugeSampleService {
    gauge_sample_repository: GaugeSampleRepository,
    cluster_repository: ClusterRepository,
    cluster_snapshot: ClusterSnapshot,
    mempool_ledger: MempoolLedger,
    feerate_diagram_snapshot: FeerateDiagramSnapshot,
    feerate_comparison_snapshot: FeerateComparisonSnapshot,
}

impl GaugeSampleService {
    pub fn new(
        gauge_sample_repository: GaugeSampleRepository,
        cluster_repository: ClusterRepository,
        cluster_snapshot: ClusterSnapshot,
        mempool_ledger: MempoolLedger,
        feerate_diagram_snapshot: FeerateDiagramSnapshot,
        feerate_comparison_snapshot: FeerateComparisonSnapshot,
    ) -> Self {
        Self {
            gauge_sample_repository,
            cluster_repository,
            cluster_snapshot,
            mempool_ledger,
            feerate_diagram_snapshot,
            feerate_comparison_snapshot,
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

    /// Samples once, inserting a new row into the `mempool_gauge_samples` table.
    pub async fn sample(&self) {
        // Both numbers read `live` under the same lock acquisition, so the gauge
        // never compares a mempool snapshot to a cluster snapshot taken at a
        // different instant.
        let (stale_member_count, live_len) = self
            .mempool_ledger
            .with_live(|live| (self.cluster_snapshot.missing_member_count(live), live.len()));
        metrics::gauge!(GAUGE_STALE_MEMBER_COUNT).set(stale_member_count as f64);

        let sampled_at = OffsetDateTime::now_utc();
        let stats = self.cluster_snapshot.stats();
        let comparison = self.compare_feerate_diagram(sampled_at).await;
        if let Some(comparison) = comparison.as_ref() {
            self.feerate_comparison_snapshot.store(comparison.clone());
        }

        let row = build_row(stats, live_len, sampled_at, comparison.as_ref());
        if let Err(e) = self.gauge_sample_repository.insert(&row).await {
            tracing::warn!("gauge sample: failed to insert sample: {e}");
        }
    }

    /// Our diagram rebuilt at the instant of the newest node diagram old enough
    /// to rebuild, next to it. `None` when there is no such diagram or the
    /// rebuild fails.
    async fn compare_feerate_diagram(
        &self,
        now: OffsetDateTime,
    ) -> Option<FeerateDiagramComparison> {
        let Some(node) = self
            .feerate_diagram_snapshot
            .latest_at_or_before(now - FEERATE_DIAGRAM_MIN_AGE)
        else {
            tracing::warn!("gauge sample: no node feerate diagram old enough to compare");
            return None;
        };
        let sampled_at = node.sampled_at?;
        if sampled_at < now - FEERATE_DIAGRAM_MAX_AGE {
            tracing::warn!(
                "gauge sample: newest node feerate diagram is from {sampled_at}, too old to compare"
            );
            return None;
        }

        let ours: Vec<DiagramSegment> = match self
            .cluster_repository
            .find_active_chunks_at(sampled_at)
            .await
        {
            Ok(rows) => rows.into_iter().map(DiagramSegment::from).collect(),
            Err(e) => {
                tracing::warn!("gauge sample: failed to rebuild our feerate diagram: {e}");
                return None;
            }
        };
        let comparison = DiagramComparison::new(&node.points, &ours);
        let unit = FeerateDiagramDiffUnit::ChunkCount;
        Some(FeerateDiagramComparison {
            sampled_at,
            ours: comparison.ours_points().to_vec(),
            node: node.points,
            missing: comparison.missing(unit),
            extra: comparison.extra(unit),
            node_total: comparison.node_total(unit),
            unit,
            missing_chunks: comparison.missing_chunks(),
            extra_chunks: comparison.extra_chunks(),
        })
    }

    /// Same range/resolution rules as `fetch_range`, projected down to one column --
    /// the table stays wide, only the response narrows.
    pub async fn metric_series(
        &self,
        metric: GaugeMetric,
        from: Option<OffsetDateTime>,
        to: Option<OffsetDateTime>,
    ) -> Result<GaugeSeries, ApiError> {
        let (resolution_secs, rows) = self.fetch_range(from, to).await?;
        Ok(GaugeSeries {
            metric,
            resolution_secs,
            points: rows
                .into_iter()
                .map(|row| GaugePoint {
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
    ) -> Result<(i64, Vec<MempoolGaugeSampleRow>), ApiError> {
        let to = to.unwrap_or_else(OffsetDateTime::now_utc);
        let from = from.unwrap_or(to - DEFAULT_RANGE);
        if from >= to {
            return Err(ApiError::BadRequest(
                "`from` must be earlier than `to`".to_string(),
            ));
        }

        let resolution_secs = pick_resolution(to - from);
        let rows = self
            .gauge_sample_repository
            .range(from, to, resolution_secs)
            .await?;
        Ok((resolution_secs, rows))
    }
}

fn metric_value(metric: GaugeMetric, row: &MempoolGaugeSampleRow) -> Option<f64> {
    match metric {
        GaugeMetric::ClusterCount => Some(row.cluster_count as f64),
        GaugeMetric::ClusteredTxCount => Some(row.clustered_tx_count as f64),
        GaugeMetric::MempoolTxCount => Some(row.mempool_tx_count as f64),
        GaugeMetric::TotalVsize => Some(vsize_from_weight(row.total_weight) as f64),
        GaugeMetric::TotalFee => Some(row.total_fee as f64),
        GaugeMetric::FeerateDiagramMissingPercentage => {
            share_of_node(row.feerate_diagram_missing, row.feerate_diagram_node_total)
        }
        GaugeMetric::FeerateDiagramExtraPercentage => {
            share_of_node(row.feerate_diagram_extra, row.feerate_diagram_node_total)
        }
    }
}

fn share_of_node(amount: Option<i64>, node_total: Option<i64>) -> Option<f64> {
    DiagramComparison::share_of_node(
        u64::try_from(amount?).ok()?,
        u64::try_from(node_total?).ok()?,
    )
}

fn build_row(
    stats: ClusterStats,
    mempool_tx_count: usize,
    sampled_at: OffsetDateTime,
    comparison: Option<&FeerateDiagramComparison>,
) -> NewMempoolGaugeSampleRow {
    NewMempoolGaugeSampleRow {
        sampled_at,
        cluster_count: stats.cluster_count as i32,
        clustered_tx_count: stats.tx_count as i32,
        mempool_tx_count: mempool_tx_count as i32,
        total_weight: stats.total_weight,
        total_fee: stats.total_fee,
        feerate_diagram_missing: comparison.map(|c| c.missing as i64),
        feerate_diagram_extra: comparison.map(|c| c.extra as i64),
        feerate_diagram_node_total: comparison.map(|c| c.node_total as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::api::MempoolFeerateDiagram;
    use testkit::fixtures::fixed_time;

    #[test]
    fn build_row_maps_each_reading_to_its_column() {
        let stats = ClusterStats {
            cluster_count: 2,
            tx_count: 3,
            total_weight: 1_198,
            total_fee: 1_200,
        };
        let sampled_at = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let comparison = FeerateDiagramComparison {
            sampled_at: sampled_at - FEERATE_DIAGRAM_MIN_AGE,
            node: Vec::new(),
            ours: Vec::new(),
            missing: 1,
            extra: 2,
            node_total: 4,
            unit: FeerateDiagramDiffUnit::ChunkCount,
            missing_chunks: Vec::new(),
            extra_chunks: Vec::new(),
        };

        let row = build_row(stats, 4, sampled_at, Some(&comparison));

        assert_eq!(row.sampled_at, sampled_at);
        assert_eq!(row.cluster_count, 2);
        assert_eq!(row.clustered_tx_count, 3);
        assert_eq!(row.mempool_tx_count, 4);
        assert_eq!(row.total_weight, 1_198);
        assert_eq!(row.total_fee, 1_200);
        assert_eq!(row.feerate_diagram_missing, Some(1));
        assert_eq!(row.feerate_diagram_extra, Some(2));
        assert_eq!(row.feerate_diagram_node_total, Some(4));
    }

    #[tokio::test]
    async fn sample_does_not_panic_when_insert_fails() {
        let repo = GaugeSampleRepository::new(testkit::postgres::inert_pool());
        let cluster_snapshot = ClusterSnapshot::default();
        let mempool_ledger = MempoolLedger::default();

        // the inert pool never connects, so a direct insert must fail...
        let row = build_row(ClusterStats::default(), 0, OffsetDateTime::now_utc(), None);
        assert!(repo.insert(&row).await.is_err());

        // ...but sample() must swallow that error, not panic.
        let service = GaugeSampleService::new(
            repo,
            ClusterRepository::new(testkit::postgres::inert_pool()),
            cluster_snapshot,
            mempool_ledger,
            FeerateDiagramSnapshot::default(),
            FeerateComparisonSnapshot::default(),
        );
        service.sample().await;
    }

    fn inert_service() -> GaugeSampleService {
        inert_service_with_diagrams(FeerateDiagramSnapshot::default())
    }

    fn inert_service_with_diagrams(diagrams: FeerateDiagramSnapshot) -> GaugeSampleService {
        GaugeSampleService::new(
            GaugeSampleRepository::new(testkit::postgres::inert_pool()),
            ClusterRepository::new(testkit::postgres::inert_pool()),
            ClusterSnapshot::default(),
            MempoolLedger::default(),
            diagrams,
            FeerateComparisonSnapshot::default(),
        )
    }

    fn diagram_sampled_at(sampled_at: OffsetDateTime) -> MempoolFeerateDiagram {
        MempoolFeerateDiagram {
            sampled_at: Some(sampled_at),
            points: Vec::new(),
        }
    }

    #[tokio::test]
    async fn no_comparison_when_the_rebuild_fails() {
        let now = fixed_time();
        let diagrams = FeerateDiagramSnapshot::default();
        diagrams.store(diagram_sampled_at(now - FEERATE_DIAGRAM_MIN_AGE));

        let service = inert_service_with_diagrams(diagrams);

        assert!(service.compare_feerate_diagram(now).await.is_none());
    }

    #[tokio::test]
    async fn metric_series_surfaces_a_repository_failure_instead_of_an_empty_series() {
        let service = inert_service();
        assert!(matches!(
            service
                .metric_series(GaugeMetric::ClusterCount, None, None)
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
        let row = MempoolGaugeSampleRow {
            sampled_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
            cluster_count: 11,
            clustered_tx_count: 22,
            mempool_tx_count: 33,
            total_weight: 173,
            total_fee: 55,
            feerate_diagram_missing: Some(1),
            feerate_diagram_extra: Some(2),
            feerate_diagram_node_total: Some(4),
        };

        assert_eq!(metric_value(GaugeMetric::ClusterCount, &row), Some(11.0));
        assert_eq!(
            metric_value(GaugeMetric::ClusteredTxCount, &row),
            Some(22.0)
        );
        assert_eq!(metric_value(GaugeMetric::MempoolTxCount, &row), Some(33.0));
        assert_eq!(
            metric_value(GaugeMetric::TotalVsize, &row),
            Some(44.0),
            "weight served as vbytes, rounded up"
        );
        assert_eq!(metric_value(GaugeMetric::TotalFee, &row), Some(55.0));
        assert_eq!(
            metric_value(GaugeMetric::FeerateDiagramMissingPercentage, &row),
            Some(0.25)
        );
        assert_eq!(
            metric_value(GaugeMetric::FeerateDiagramExtraPercentage, &row),
            Some(0.5)
        );
    }

    #[test]
    fn feerate_percentages_have_no_value_without_a_node_total() {
        let mut row = MempoolGaugeSampleRow {
            sampled_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
            cluster_count: 0,
            clustered_tx_count: 0,
            mempool_tx_count: 0,
            total_weight: 0,
            total_fee: 0,
            feerate_diagram_missing: None,
            feerate_diagram_extra: None,
            feerate_diagram_node_total: None,
        };
        for metric in [
            GaugeMetric::FeerateDiagramMissingPercentage,
            GaugeMetric::FeerateDiagramExtraPercentage,
        ] {
            assert_eq!(metric_value(metric, &row), None, "{metric:?}, no reading");
        }

        row.feerate_diagram_missing = Some(0);
        row.feerate_diagram_extra = Some(3);
        row.feerate_diagram_node_total = Some(0);
        for metric in [
            GaugeMetric::FeerateDiagramMissingPercentage,
            GaugeMetric::FeerateDiagramExtraPercentage,
        ] {
            assert_eq!(
                metric_value(metric, &row),
                None,
                "{metric:?}, empty node diagram"
            );
        }
    }
}
