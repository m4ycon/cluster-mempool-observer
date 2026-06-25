use crate::db::{ClusterRepository, DbPool, MempoolDeltaRepository, TransactionRepository};
use crate::services::bootstrap::BootstrapService;
use crate::services::cluster::ClusterService;
use crate::services::mempool::MempoolService;
use crate::services::pubsub::PubSubService;
use axum::Router;
use axum::extract::FromRef;
use observer::clients::{Clients, rpc_client::RpcClient, zmq_client::ZmqClient};
use observer::infra::config::Config as ObserverConfig;
use observer::retrievers::{ClusterRpcRetriever, MempoolRetriever, TransactionRpcRetriever};
use observer::snapshot::MempoolSnapshot;
use shared::pubsub::PubSub;

pub type AppMempoolService = MempoolService;

#[derive(Clone)]
pub struct AppState {
    pub mempool_retriever: MempoolRetriever,
    pub mempool_service: AppMempoolService,
    pub bootstrap_service: BootstrapService,
}

impl AppState {
    pub fn build(config: &ObserverConfig, db_pool: DbPool) -> (Self, MempoolSnapshot, Clients) {
        // weirdos
        let snapshot = MempoolSnapshot::default();

        // clients
        let clients = Clients {
            pubsub: PubSub::new(),
            rpc: RpcClient::new(&config.rpc).expect("failed to initialize RPC client"),
            zmq: ZmqClient::new(&config.zmq),
        };

        // repositories
        let cluster_repository = ClusterRepository::new(db_pool.clone());
        let mempool_delta_repository = MempoolDeltaRepository::new(db_pool.clone());
        let transaction_repository = TransactionRepository::new(db_pool);

        // retrievers
        let transaction_retriever = TransactionRpcRetriever::new(clients.rpc.clone());
        let cluster_retriever = ClusterRpcRetriever::new(clients.rpc.clone());
        let mempool_retriever = MempoolRetriever::new(clients.rpc.clone(), snapshot.clone());

        // services
        let pubsub_service = PubSubService::new(clients.pubsub.clone());
        let cluster_service = ClusterService::new(
            cluster_repository,
            transaction_repository.clone(),
            cluster_retriever,
        );
        let mempool_service = MempoolService::new(
            mempool_delta_repository.clone(),
            transaction_repository,
            transaction_retriever,
            cluster_service,
            pubsub_service,
        );
        let bootstrap_service = BootstrapService::new(
            mempool_delta_repository,
            mempool_retriever.clone(),
            mempool_service.clone(),
        );

        let state = Self {
            mempool_retriever,
            mempool_service,
            bootstrap_service,
        };
        (state, snapshot, clients)
    }
}

impl FromRef<AppState> for MempoolRetriever {
    fn from_ref(state: &AppState) -> Self {
        state.mempool_retriever.clone()
    }
}

impl FromRef<AppState> for AppMempoolService {
    fn from_ref(state: &AppState) -> Self {
        state.mempool_service.clone()
    }
}

pub type AppRouter = Router<AppState>;
