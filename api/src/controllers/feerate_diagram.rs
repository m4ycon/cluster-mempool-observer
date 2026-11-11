use crate::infra::state::{AppFeerateDiagramService, AppRouter};
use axum::Json;
use axum::extract::State;
use axum::routing::get;
use shared::api::{FeerateDiagramComparison, MempoolFeerateDiagram};

pub trait FeerateDiagramControllerRouter {
    fn add_feerate_diagram_routes(self) -> Self;
}

impl FeerateDiagramControllerRouter for AppRouter {
    fn add_feerate_diagram_routes(self) -> Self {
        self.route("/mempool/feerate-diagram", get(feerate_diagram))
            .route(
                "/mempool/feerate-diagram/comparison",
                get(feerate_diagram_comparison),
            )
    }
}

/// Returns the watcher's latest raw cumulative feerate diagram.
async fn feerate_diagram(
    State(service): State<AppFeerateDiagramService>,
) -> Json<MempoolFeerateDiagram> {
    Json(service.current())
}

/// Returns the last node-vs-ours comparison, `null` before the first one: no
/// data yet is not an error, as with the raw diagram's cold start.
async fn feerate_diagram_comparison(
    State(service): State<AppFeerateDiagramService>,
) -> Json<Option<FeerateDiagramComparison>> {
    Json(service.comparison())
}
