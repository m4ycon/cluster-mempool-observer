use crate::db::{DbPool, MempoolDeltaRepository, TransactionRepository};
use crate::services::pubsub::PubSubService;
use axum::Router;
use axum::extract::FromRef;
use observer::clients::Clients;
use observer::retrievers::{MempoolRetriever, TransactionRpcRetriever};
use observer::snapshot::MempoolSnapshot;

#[derive(Clone)]
pub struct AppState {
    pub pubsub: PubSubService,
    pub mempool_retriever: MempoolRetriever,
    pub transaction_retriever: TransactionRpcRetriever,
    pub transaction_repository: TransactionRepository,
    pub mempool_delta_repository: MempoolDeltaRepository,
}

impl AppState {
    pub fn new(
        pubsub: PubSubService,
        mempool_retriever: MempoolRetriever,
        transaction_retriever: TransactionRpcRetriever,
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

    pub fn build(clients: Clients, db_pool: DbPool) -> (Self, MempoolSnapshot) {
        let snapshot = MempoolSnapshot::default();
        let state = Self::new(
            PubSubService::new(clients.pubsub),
            MempoolRetriever::new(clients.rpc.clone(), snapshot.clone()),
            TransactionRpcRetriever::new(clients.rpc),
            TransactionRepository::new(db_pool.clone()),
            MempoolDeltaRepository::new(db_pool),
        );
        (state, snapshot)
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

impl FromRef<AppState> for TransactionRpcRetriever {
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
