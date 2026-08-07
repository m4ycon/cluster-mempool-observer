use crate::infra::state::AppRouter;
use axum::Json;
use axum::extract::State;
use axum::routing::get;
use observer::retrievers::MempoolRetriever;
use std::collections::HashSet;

pub trait MempoolControllerRouter {
    fn add_mempool_routes(self) -> Self;
}

impl MempoolControllerRouter for AppRouter {
    fn add_mempool_routes(self) -> Self {
        self.route("/mempool/txids", get(mempool_txids))
    }
}

/// Returns the watcher's current mempool txid set.
async fn mempool_txids(State(mempool_retriever): State<MempoolRetriever>) -> Json<HashSet<String>> {
    Json(mempool_retriever.mempool_txids())
}
