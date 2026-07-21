use crate::db::{
    BlockRepository, ClusterMembershipRepository, ClusterRepository, DbPool,
    MempoolDeltaRepository, TransactionRepository,
};
use crate::services::block::BlockService;
use crate::services::bootstrap::BootstrapService;
use crate::services::cluster::ClusterService;
use crate::services::cluster_delta::ClusterDeltaService;
use crate::services::mempool::MempoolService;
use crate::services::pubsub::PubSubService;
use axum::Router;
use axum::extract::FromRef;
use observer::clients::{Clients, rpc_client::RpcClient, zmq_client::ZmqClient};
use observer::infra::config::Config as ObserverConfig;
use observer::retrievers::{
    BlockRpcRetriever, ClusterRpcRetriever, MempoolRetriever, TransactionRpcRetriever,
};
use shared::pubsub::PubSub;
use shared::snapshot::ClusterSnapshot;
use shared::snapshot::MempoolSnapshot;

pub type AppMempoolService = MempoolService;
pub type AppBlockService = BlockService;
pub type AppClusterService = ClusterService;

#[derive(Clone)]
pub struct AppState {
    pub mempool_retriever: MempoolRetriever,
    pub mempool_service: AppMempoolService,
    pub block_service: AppBlockService,
    pub cluster_service: AppClusterService,
    pub bootstrap_service: BootstrapService,
}

impl AppState {
    pub fn build(config: &ObserverConfig, db_pool: DbPool) -> (Self, MempoolSnapshot, Clients) {
        // weirdos
        let mempool_snapshot = MempoolSnapshot::default();
        let cluster_snapshot = ClusterSnapshot::default();

        // clients
        let clients = Clients {
            pubsub: PubSub::new(),
            rpc: RpcClient::new(&config.rpc).expect("failed to initialize RPC client"),
            zmq: ZmqClient::new(&config.zmq),
        };

        // repositories
        let cluster_repository = ClusterRepository::new(db_pool.clone());
        let membership_repository = ClusterMembershipRepository::new(db_pool.clone());
        let mempool_delta_repository = MempoolDeltaRepository::new(db_pool.clone());
        let block_repository = BlockRepository::new(db_pool.clone());
        let transaction_repository = TransactionRepository::new(db_pool);

        // retrievers
        let transaction_retriever = TransactionRpcRetriever::new(clients.rpc.clone());
        let cluster_retriever = ClusterRpcRetriever::new(clients.rpc.clone());
        let block_retriever = BlockRpcRetriever::new(clients.rpc.clone());
        let mempool_retriever =
            MempoolRetriever::new(clients.rpc.clone(), mempool_snapshot.clone());

        // services
        let pubsub_service = PubSubService::new(clients.pubsub.clone());
        let cluster_delta_service =
            ClusterDeltaService::new(cluster_snapshot, pubsub_service.clone());
        let cluster_service = ClusterService::new(
            cluster_repository,
            transaction_repository.clone(),
            membership_repository,
            cluster_retriever,
            cluster_delta_service,
        );
        let block_service = BlockService::new(
            block_repository,
            transaction_repository.clone(),
            mempool_delta_repository.clone(),
            cluster_service.clone(),
            block_retriever,
            pubsub_service.clone(),
        );
        let mempool_service = MempoolService::new(
            mempool_delta_repository.clone(),
            transaction_repository,
            transaction_retriever,
            cluster_service.clone(),
            pubsub_service,
        );
        let bootstrap_service = BootstrapService::new(
            mempool_delta_repository,
            mempool_retriever.clone(),
            mempool_service.clone(),
            block_service.clone(),
            cluster_service.clone(),
        );

        let state = Self {
            mempool_retriever,
            mempool_service,
            block_service,
            cluster_service,
            bootstrap_service,
        };
        (state, mempool_snapshot, clients)
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

impl FromRef<AppState> for AppClusterService {
    fn from_ref(state: &AppState) -> Self {
        state.cluster_service.clone()
    }
}

pub type AppRouter = Router<AppState>;
