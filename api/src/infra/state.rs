use crate::services::block::BlockService;
use crate::services::bootstrap::BootstrapService;
use crate::services::cluster::ClusterService;
use crate::services::home::HomeService;
use crate::services::mempool::MempoolService;
use axum::Router;
use axum::extract::FromRef;
use observer::retrievers::MempoolRetriever;

pub type AppMempoolService = MempoolService;
pub type AppBlockService = BlockService;
pub type AppClusterService = ClusterService;
pub type AppHomeService = HomeService;

#[derive(Clone)]
pub struct AppState {
    pub mempool_retriever: MempoolRetriever,
    pub mempool_service: AppMempoolService,
    pub block_service: AppBlockService,
    pub cluster_service: AppClusterService,
    pub home_service: AppHomeService,
    pub bootstrap_service: BootstrapService,
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

impl FromRef<AppState> for AppHomeService {
    fn from_ref(state: &AppState) -> Self {
        state.home_service.clone()
    }
}

pub type AppRouter = Router<AppState>;
