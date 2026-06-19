use crate::db::{MempoolDeltaRepository, TransactionRepository};
use crate::services::pubsub::PubSubService;
use axum::Router;
use axum::extract::FromRef;
use observer::retrievers::{MempoolRetriever, TransactionRetriever};

#[derive(Clone)]
pub struct AppState {
    pub pubsub: PubSubService,
    pub mempool_retriever: MempoolRetriever,
    pub transaction_retriever: TransactionRetriever,
    pub transaction_repository: TransactionRepository,
    pub mempool_delta_repository: MempoolDeltaRepository,
}

impl AppState {
    pub fn new(
        pubsub: PubSubService,
        mempool_retriever: MempoolRetriever,
        transaction_retriever: TransactionRetriever,
        transaction_repository: TransactionRepository,
        mempool_delta_repository: MempoolDeltaRepository,
    ) -> Self {
        Self {
            pubsub,
            mempool_retriever,
            transaction_retriever,
            transaction_repository,
            mempool_delta_repository,
        }
    }
}

impl FromRef<AppState> for PubSubService {
    fn from_ref(state: &AppState) -> Self {
        state.pubsub.clone()
    }
}

impl FromRef<AppState> for MempoolRetriever {
    fn from_ref(state: &AppState) -> Self {
        state.mempool_retriever.clone()
    }
}

impl FromRef<AppState> for TransactionRetriever {
    fn from_ref(state: &AppState) -> Self {
        state.transaction_retriever.clone()
    }
}

impl FromRef<AppState> for TransactionRepository {
    fn from_ref(state: &AppState) -> Self {
        state.transaction_repository.clone()
    }
}

impl FromRef<AppState> for MempoolDeltaRepository {
    fn from_ref(state: &AppState) -> Self {
        state.mempool_delta_repository.clone()
    }
}

pub type AppRouter = Router<AppState>;
