use crate::db::Repos;
use crate::infra::readiness::Readiness;
use crate::infra::state::AppState;
use crate::services::block::BlockService;
use crate::services::bootstrap::BootstrapService;
use crate::services::cluster::ClusterService;
use crate::services::cluster_delta::ClusterDeltaService;
use crate::services::feerate_diagram::FeerateDiagramService;
use crate::services::home::HomeService;
use crate::services::mempool::MempoolService;
use crate::services::mempool_reconciler::MempoolReconciler;
use crate::services::node_health::NodeHealthService;
use crate::services::node_status::NodeStatusService;
use crate::services::pubsub::PubSubService;
use crate::services::snapshot::SnapshotService;
use crate::services::system_event::SystemEventService;
use crate::services::transaction::TransactionService;
use crate::services::tx_backfill::{TxBackfillConsumer, TxBackfillQueue};
use observer::clients::Clients;
use observer::retrievers::{
    BlockRetriever, BlockRpcRetriever, ClusterRetriever, ClusterRpcRetriever, MempoolRetriever,
    NetworkRpcRetriever, TransactionRetriever, TransactionRpcRetriever,
};
use shared::snapshot::ClusterSnapshot;
use shared::snapshot::FeerateDiagramSnapshot;
use shared::snapshot::MempoolLedger;
use shared::snapshot::MempoolSnapshot;

const DEFAULT_TX_BACKFILL_QUEUE_CAPACITY: usize = 100_000;

#[derive(Clone)]
pub struct Deps<
    TR: TransactionRetriever = TransactionRpcRetriever,
    CR: ClusterRetriever = ClusterRpcRetriever,
    BR: BlockRetriever = BlockRpcRetriever,
> {
    pub repos: Repos,
    pub pubsub: PubSubService,
    pub mempool_snapshot: MempoolSnapshot,
    pub mempool_ledger: MempoolLedger,
    pub cluster_snapshot: ClusterSnapshot,
    pub feerate_diagram_snapshot: FeerateDiagramSnapshot,
    pub mempool_retriever: MempoolRetriever,
    pub transaction_retriever: TR,
    pub cluster_retriever: CR,
    pub block_retriever: BR,
    pub network_retriever: NetworkRpcRetriever,
    pub node_health_service: NodeHealthService<NetworkRpcRetriever>,
    pub readiness: Readiness,
    pub tx_backfill_queue: TxBackfillQueue,
}

impl Deps {
    pub fn new(repos: Repos, clients: &Clients) -> Self {
        Self::with_queue_capacity(repos, clients, DEFAULT_TX_BACKFILL_QUEUE_CAPACITY)
    }

    pub fn with_queue_capacity(repos: Repos, clients: &Clients, queue_capacity: usize) -> Self {
        let pubsub = PubSubService::new(clients.pubsub.clone());
        let mempool_snapshot = MempoolSnapshot::default();
        let mempool_ledger = MempoolLedger::default();
        let cluster_snapshot = ClusterSnapshot::default();
        let feerate_diagram_snapshot = FeerateDiagramSnapshot::default();
        let mempool_retriever = MempoolRetriever::new(clients.rpc.clone());
        let transaction_retriever = TransactionRpcRetriever::new(clients.rpc.clone());
        let cluster_retriever = ClusterRpcRetriever::new(clients.rpc.clone());
        let block_retriever = BlockRpcRetriever::new(clients.rpc.clone());
        let network_retriever = NetworkRpcRetriever::new(clients.rpc.clone());
        let node_health_service = NodeHealthService::new(
            SystemEventService::new(repos.system_event.clone()),
            network_retriever.clone(),
        );

        Self {
            repos,
            pubsub,
            mempool_snapshot,
            mempool_ledger,
            cluster_snapshot,
            feerate_diagram_snapshot,
            mempool_retriever,
            transaction_retriever,
            cluster_retriever,
            block_retriever,
            network_retriever,
            node_health_service,
            readiness: Readiness::default(),
            tx_backfill_queue: TxBackfillQueue::new(queue_capacity),
        }
    }

    pub fn app_state(&self) -> AppState {
        AppState {
            readiness: self.readiness.clone(),
            mempool_ledger: self.mempool_ledger.clone(),
            mempool_service: self.mempool_service(),
            block_service: self.block_service(),
            cluster_service: self.cluster_service(),
            home_service: self.home_service(),
            snapshot_service: self.snapshot_service(),
            feerate_diagram_service: self.feerate_diagram_service(),
            bootstrap_service: self.bootstrap_service(),
            system_event_service: self.system_event_service(),
            transaction_service: self.transaction_service(),
        }
    }

    pub fn bootstrap_service(&self) -> BootstrapService {
        BootstrapService::new(
            self.repos.mempool_delta.clone(),
            self.mempool_retriever.clone(),
            self.repos.transaction.clone(),
            self.mempool_ledger.clone(),
            self.block_service(),
            self.cluster_service(),
            self.system_event_service(),
            self.node_status_service(),
        )
    }
}

impl<TR: TransactionRetriever, CR: ClusterRetriever, BR: BlockRetriever> Deps<TR, CR, BR> {
    pub fn cluster_delta_service(&self) -> ClusterDeltaService {
        ClusterDeltaService::new(self.cluster_snapshot.clone(), self.pubsub.clone())
    }

    pub fn cluster_service(&self) -> ClusterService<CR> {
        ClusterService::new(
            self.repos.cluster.clone(),
            self.repos.cluster_membership.clone(),
            self.cluster_retriever.clone(),
            self.cluster_delta_service(),
            self.mempool_ledger.clone(),
        )
    }

    pub fn block_service(&self) -> BlockService<BR, CR> {
        BlockService::new(
            self.repos.block.clone(),
            self.repos.transaction.clone(),
            self.mempool_ledger.clone(),
            self.cluster_service(),
            self.block_retriever.clone(),
            self.pubsub.clone(),
        )
    }

    pub fn mempool_service(&self) -> MempoolService<CR> {
        MempoolService::new(
            self.repos.mempool_admission.clone(),
            self.repos.mempool_delta.clone(),
            self.repos.transaction.clone(),
            self.tx_backfill_queue.clone(),
            self.cluster_service(),
            self.pubsub.clone(),
        )
    }

    pub fn mempool_reconciler(&self) -> MempoolReconciler<CR> {
        MempoolReconciler::new(
            self.mempool_ledger.clone(),
            self.repos.mempool_ledger.clone(),
            self.tx_backfill_queue.clone(),
            self.cluster_service(),
            self.pubsub.clone(),
        )
    }

    pub fn home_service(&self) -> HomeService<CR> {
        HomeService::new(
            self.repos.block.clone(),
            self.repos.mempool_delta.clone(),
            self.mempool_ledger.clone(),
            self.cluster_service(),
            self.pubsub.clone(),
        )
    }

    pub fn snapshot_service(&self) -> SnapshotService {
        SnapshotService::new(
            self.repos.snapshot.clone(),
            self.cluster_snapshot.clone(),
            self.mempool_ledger.clone(),
        )
    }

    pub fn transaction_service(&self) -> TransactionService {
        TransactionService::new(self.repos.transaction.clone())
    }

    pub fn tx_backfill_consumer(&self) -> TxBackfillConsumer<TR>
    where
        TR: 'static,
    {
        TxBackfillConsumer::new(
            self.repos.transaction.clone(),
            self.transaction_retriever.clone(),
            self.tx_backfill_queue.clone(),
            self.mempool_ledger.clone(),
        )
    }

    pub fn feerate_diagram_service(&self) -> FeerateDiagramService {
        FeerateDiagramService::new(self.feerate_diagram_snapshot.clone())
    }

    pub fn system_event_service(&self) -> SystemEventService {
        SystemEventService::new(self.repos.system_event.clone())
    }

    pub fn node_status_service(&self) -> NodeStatusService<NetworkRpcRetriever> {
        NodeStatusService::new(self.pubsub.clone(), self.node_health_service.clone())
    }

    /// Swaps the transaction retriever, e.g. for a mock in tests.
    pub fn with_transaction_retriever<T2: TransactionRetriever>(self, r: T2) -> Deps<T2, CR, BR> {
        Deps {
            repos: self.repos,
            pubsub: self.pubsub,
            mempool_snapshot: self.mempool_snapshot,
            mempool_ledger: self.mempool_ledger,
            cluster_snapshot: self.cluster_snapshot,
            feerate_diagram_snapshot: self.feerate_diagram_snapshot,
            mempool_retriever: self.mempool_retriever,
            transaction_retriever: r,
            cluster_retriever: self.cluster_retriever,
            block_retriever: self.block_retriever,
            network_retriever: self.network_retriever,
            node_health_service: self.node_health_service,
            readiness: self.readiness,
            tx_backfill_queue: self.tx_backfill_queue,
        }
    }

    /// Swaps the cluster retriever, e.g. for a mock in tests.
    pub fn with_cluster_retriever<C2: ClusterRetriever>(self, r: C2) -> Deps<TR, C2, BR> {
        Deps {
            repos: self.repos,
            pubsub: self.pubsub,
            mempool_snapshot: self.mempool_snapshot,
            mempool_ledger: self.mempool_ledger,
            cluster_snapshot: self.cluster_snapshot,
            feerate_diagram_snapshot: self.feerate_diagram_snapshot,
            mempool_retriever: self.mempool_retriever,
            transaction_retriever: self.transaction_retriever,
            cluster_retriever: r,
            block_retriever: self.block_retriever,
            network_retriever: self.network_retriever,
            node_health_service: self.node_health_service,
            readiness: self.readiness,
            tx_backfill_queue: self.tx_backfill_queue,
        }
    }

    /// Swaps the block retriever, e.g. for a mock in tests.
    pub fn with_block_retriever<B2: BlockRetriever>(self, r: B2) -> Deps<TR, CR, B2> {
        Deps {
            repos: self.repos,
            pubsub: self.pubsub,
            mempool_snapshot: self.mempool_snapshot,
            mempool_ledger: self.mempool_ledger,
            cluster_snapshot: self.cluster_snapshot,
            feerate_diagram_snapshot: self.feerate_diagram_snapshot,
            mempool_retriever: self.mempool_retriever,
            transaction_retriever: self.transaction_retriever,
            cluster_retriever: self.cluster_retriever,
            block_retriever: r,
            network_retriever: self.network_retriever,
            node_health_service: self.node_health_service,
            readiness: self.readiness,
            tx_backfill_queue: self.tx_backfill_queue,
        }
    }

    pub fn with_mempool_retriever(mut self, r: MempoolRetriever) -> Self {
        self.mempool_retriever = r;
        self
    }

    pub fn with_pubsub(mut self, pubsub: PubSubService) -> Self {
        self.pubsub = pubsub;
        self
    }

    pub fn with_cluster_snapshot(mut self, snapshot: ClusterSnapshot) -> Self {
        self.cluster_snapshot = snapshot;
        self
    }

    pub fn with_feerate_diagram_snapshot(mut self, snapshot: FeerateDiagramSnapshot) -> Self {
        self.feerate_diagram_snapshot = snapshot;
        self
    }
}
