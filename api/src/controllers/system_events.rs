use crate::error::ApiError;
use crate::infra::state::{AppRouter, AppSystemEventService};
use axum::Json;
use axum::extract::{Query, State};
use axum::routing::get;
use serde::Deserialize;
use time::OffsetDateTime;

pub trait SystemEventsControllerRouter {
    fn add_system_event_routes(self) -> Self;
}

impl SystemEventsControllerRouter for AppRouter {
    fn add_system_event_routes(self) -> Self {
        self.route("/system-events", get(system_events))
    }
}

#[derive(Debug, Deserialize)]
struct SystemEventsQuery {
    #[serde(default, with = "time::serde::rfc3339::option")]
    from: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    to: Option<OffsetDateTime>,
}

/// Lists the system event lifecycle log, for filtering restart/bootstrap artifacts out of history.
async fn system_events(
    State(service): State<AppSystemEventService>,
    Query(query): Query<SystemEventsQuery>,
) -> Result<Json<Vec<shared::events::SystemEvent>>, ApiError> {
    let events = service.list(query.from, query.to).await?;
    Ok(Json(events))
}
