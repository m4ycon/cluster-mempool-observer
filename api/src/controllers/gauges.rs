use crate::error::ApiError;
use crate::infra::state::{AppGaugeSampleService, AppRouter};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use serde::Deserialize;
use shared::api::{GaugeMetric, GaugeSeries};
use time::OffsetDateTime;

pub trait GaugesControllerRouter {
    fn add_gauge_routes(self) -> Self;
}

impl GaugesControllerRouter for AppRouter {
    fn add_gauge_routes(self) -> Self {
        self.route("/mempool/gauges/{metric}", get(mempool_gauge_metric))
    }
}

#[derive(Debug, Deserialize)]
struct GaugeRangeQuery {
    #[serde(default, with = "time::serde::rfc3339::option")]
    from: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    to: Option<OffsetDateTime>,
}

/// Single-metric projection of the mempool/cluster history, for charts that need only one line.
async fn mempool_gauge_metric(
    State(service): State<AppGaugeSampleService>,
    Path(metric): Path<GaugeMetric>,
    Query(range): Query<GaugeRangeQuery>,
) -> Result<Json<GaugeSeries>, ApiError> {
    let series = service.metric_series(metric, range.from, range.to).await?;
    Ok(Json(series))
}
