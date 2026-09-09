use crate::infra::state::AppRouter;
use axum::Json;
use axum::extract::State;
use axum::routing::get;
use shared::snapshot::MempoolLedger;
use std::collections::HashSet;

pub trait MempoolControllerRouter {
    fn add_mempool_routes(self) -> Self;
}

impl MempoolControllerRouter for AppRouter {
    fn add_mempool_routes(self) -> Self {
        self.route("/mempool/txids", get(mempool_txids))
    }
}

/// Returns the ledger's current mempool txid set.
async fn mempool_txids(State(mempool_ledger): State<MempoolLedger>) -> Json<HashSet<String>> {
    Json(mempool_ledger.clone_live_snapshot())
}
