use crate::error::ApiError;
use crate::infra::state::{AppCounterSampleService, AppRouter};
use axum::Json;
use axum::extract::{Query, State};
use axum::routing::get;
use serde::Deserialize;
use shared::api::CounterSeries;
use time::OffsetDateTime;

pub trait CountersControllerRouter {
    fn add_counter_routes(self) -> Self;
}

impl CountersControllerRouter for AppRouter {
    fn add_counter_routes(self) -> Self {
        self.route("/mempool/counters", get(mempool_counters))
    }
}

#[derive(Debug, Deserialize)]
struct CounterRangeQuery {
    #[serde(default, with = "time::serde::rfc3339::option")]
    from: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    to: Option<OffsetDateTime>,
}

async fn mempool_counters(
    State(service): State<AppCounterSampleService>,
    Query(range): Query<CounterRangeQuery>,
) -> Result<Json<CounterSeries>, ApiError> {
    let series = service.series(range.from, range.to).await?;
    Ok(Json(series))
}
