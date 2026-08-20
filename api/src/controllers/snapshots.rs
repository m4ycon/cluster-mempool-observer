use crate::error::ApiError;
use crate::infra::state::{AppRouter, AppSnapshotService};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use serde::Deserialize;
use shared::api::{MempoolMetricSeries, SnapshotMetric};
use time::OffsetDateTime;

pub trait SnapshotsControllerRouter {
    fn add_snapshot_routes(self) -> Self;
}

impl SnapshotsControllerRouter for AppRouter {
    fn add_snapshot_routes(self) -> Self {
        self.route("/mempool/snapshots/{metric}", get(mempool_snapshot_metric))
    }
}

#[derive(Debug, Deserialize)]
struct SnapshotRangeQuery {
    #[serde(default, with = "time::serde::rfc3339::option")]
    from: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    to: Option<OffsetDateTime>,
}

/// Single-metric projection of the mempool/cluster history, for charts that need only one line.
async fn mempool_snapshot_metric(
    State(service): State<AppSnapshotService>,
    Path(metric): Path<SnapshotMetric>,
    Query(range): Query<SnapshotRangeQuery>,
) -> Result<Json<MempoolMetricSeries>, ApiError> {
    let series = service.metric_series(metric, range.from, range.to).await?;
    Ok(Json(series))
}
