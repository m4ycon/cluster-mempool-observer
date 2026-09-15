use crate::infra::readiness::Readiness;
use crate::services::block::BlockService;
use crate::services::bootstrap::BootstrapService;
use crate::services::cluster::ClusterService;
use crate::services::feerate_diagram::FeerateDiagramService;
use crate::services::gauge_sample::GaugeSampleService;
use crate::services::home::HomeService;
use crate::services::mempool::MempoolService;
use crate::services::system_event::SystemEventService;
use crate::services::transaction::TransactionService;
use axum::Router;
use axum::extract::FromRef;
use shared::snapshot::MempoolLedger;

pub type AppMempoolService = MempoolService;
pub type AppBlockService = BlockService;
pub type AppClusterService = ClusterService;
pub type AppHomeService = HomeService;
pub type AppGaugeSampleService = GaugeSampleService;
pub type AppFeerateDiagramService = FeerateDiagramService;
pub type AppSystemEventService = SystemEventService;
pub type AppTransactionService = TransactionService;

#[derive(Clone)]
pub struct AppState {
    pub mempool_ledger: MempoolLedger,
    pub mempool_service: AppMempoolService,
    pub block_service: AppBlockService,
    pub cluster_service: AppClusterService,
    pub home_service: AppHomeService,
    pub gauge_sample_service: AppGaugeSampleService,
    pub feerate_diagram_service: AppFeerateDiagramService,
    pub bootstrap_service: BootstrapService,
    pub readiness: Readiness,
    pub system_event_service: AppSystemEventService,
    pub transaction_service: AppTransactionService,
}

impl FromRef<AppState> for Readiness {
    fn from_ref(state: &AppState) -> Self {
        state.readiness.clone()
    }
}

impl FromRef<AppState> for MempoolLedger {
    fn from_ref(state: &AppState) -> Self {
        state.mempool_ledger.clone()
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

impl FromRef<AppState> for AppGaugeSampleService {
    fn from_ref(state: &AppState) -> Self {
        state.gauge_sample_service.clone()
    }
}

impl FromRef<AppState> for AppFeerateDiagramService {
    fn from_ref(state: &AppState) -> Self {
        state.feerate_diagram_service.clone()
    }
}

impl FromRef<AppState> for AppSystemEventService {
    fn from_ref(state: &AppState) -> Self {
        state.system_event_service.clone()
    }
}

impl FromRef<AppState> for AppTransactionService {
    fn from_ref(state: &AppState) -> Self {
        state.transaction_service.clone()
    }
}

pub type AppRouter = Router<AppState>;
