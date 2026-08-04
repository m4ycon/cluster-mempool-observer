use crate::infra::readiness::{HealthReport, Readiness};
use crate::infra::state::AppRouter;
use axum::Json;
use axum::extract::State;
use axum::routing::get;

pub trait HealthControllerRouter {
    fn add_health_routes(self) -> Self;
}

impl HealthControllerRouter for AppRouter {
    fn add_health_routes(self) -> Self {
        self.route("/health", get(health))
    }
}

async fn health(State(readiness): State<Readiness>) -> Json<HealthReport> {
    Json(readiness.report())
}
