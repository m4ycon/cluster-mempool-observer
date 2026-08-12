use crate::infra::readiness::Readiness;
use crate::services::block::BlockService;
use crate::services::bootstrap::BootstrapService;
use crate::services::cluster::ClusterService;
use crate::services::feerate_diagram::FeerateDiagramService;
use crate::services::home::HomeService;
use crate::services::mempool::MempoolService;
use crate::services::snapshot::SnapshotService;
use axum::Router;
use axum::extract::FromRef;
use observer::retrievers::MempoolRetriever;

pub type AppMempoolService = MempoolService;
pub type AppBlockService = BlockService;
pub type AppClusterService = ClusterService;
pub type AppHomeService = HomeService;
pub type AppSnapshotService = SnapshotService;
pub type AppFeerateDiagramService = FeerateDiagramService;

#[derive(Clone)]
pub struct AppState {
    pub mempool_retriever: MempoolRetriever,
    pub mempool_service: AppMempoolService,
    pub block_service: AppBlockService,
    pub cluster_service: AppClusterService,
    pub home_service: AppHomeService,
    pub snapshot_service: AppSnapshotService,
    pub feerate_diagram_service: AppFeerateDiagramService,
    pub bootstrap_service: BootstrapService,
    pub readiness: Readiness,
}

impl FromRef<AppState> for Readiness {
    fn from_ref(state: &AppState) -> Self {
        state.readiness.clone()
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

impl FromRef<AppState> for AppHomeService {
    fn from_ref(state: &AppState) -> Self {
        state.home_service.clone()
    }
}

impl FromRef<AppState> for AppSnapshotService {
    fn from_ref(state: &AppState) -> Self {
        state.snapshot_service.clone()
    }
}

impl FromRef<AppState> for AppFeerateDiagramService {
    fn from_ref(state: &AppState) -> Self {
        state.feerate_diagram_service.clone()
    }
}

pub type AppRouter = Router<AppState>;
