use crate::services::pubsub::PubSubService;
use axum::Router;
use axum::extract::FromRef;
use observer::retrievers::{MempoolRetriever, TransactionRetriever};

#[derive(Clone)]
pub struct AppState {
    pub pubsub: PubSubService,
    pub mempool_retriever: MempoolRetriever,
    pub transaction_retriever: TransactionRetriever,
}

impl AppState {
    pub fn new(
        pubsub: PubSubService,
        mempool_retriever: MempoolRetriever,
        transaction_retriever: TransactionRetriever,
    ) -> Self {
        Self {
            pubsub,
            mempool_retriever,
            transaction_retriever,
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

pub type AppRouter = Router<AppState>;
