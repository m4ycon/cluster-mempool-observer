use crate::infra::state::{AppFeerateDiagramService, AppRouter};
use axum::Json;
use axum::extract::State;
use axum::routing::get;
use shared::api::MempoolFeerateDiagram;

pub trait FeerateDiagramControllerRouter {
    fn add_feerate_diagram_routes(self) -> Self;
}

impl FeerateDiagramControllerRouter for AppRouter {
    fn add_feerate_diagram_routes(self) -> Self {
        self.route("/mempool/feerate-diagram", get(feerate_diagram))
    }
}

/// Returns the watcher's latest raw cumulative feerate diagram.
async fn feerate_diagram(
    State(service): State<AppFeerateDiagramService>,
) -> Json<MempoolFeerateDiagram> {
    Json(service.current())
}
