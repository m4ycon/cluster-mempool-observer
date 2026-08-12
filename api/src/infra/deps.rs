use crate::db::Repos;
use crate::infra::readiness::Readiness;
use crate::infra::state::AppState;
use crate::services::block::BlockService;
use crate::services::bootstrap::BootstrapService;
use crate::services::cluster::ClusterService;
use crate::services::cluster_delta::ClusterDeltaService;
use crate::services::home::HomeService;
use crate::services::mempool::MempoolService;
use crate::services::pubsub::PubSubService;
use crate::services::snapshot::SnapshotService;
use observer::clients::Clients;
use observer::retrievers::{
    BlockRetriever, BlockRpcRetriever, ClusterRetriever, ClusterRpcRetriever, MempoolRetriever,
    TransactionRetriever, TransactionRpcRetriever,
};
use shared::snapshot::ClusterSnapshot;
use shared::snapshot::MempoolSnapshot;

#[derive(Clone)]
pub struct Deps<
    TR: TransactionRetriever = TransactionRpcRetriever,
    CR: ClusterRetriever = ClusterRpcRetriever,
    BR: BlockRetriever = BlockRpcRetriever,
> {
    pub repos: Repos,
    pub pubsub: PubSubService,
    pub mempool_snapshot: MempoolSnapshot,
    pub cluster_snapshot: ClusterSnapshot,
    pub mempool_retriever: MempoolRetriever,
    pub transaction_retriever: TR,
    pub cluster_retriever: CR,
    pub block_retriever: BR,
    pub readiness: Readiness,
}

impl Deps {
    pub fn new(repos: Repos, clients: &Clients) -> Self {
        let pubsub = PubSubService::new(clients.pubsub.clone());
        let mempool_snapshot = MempoolSnapshot::default();
        let cluster_snapshot = ClusterSnapshot::default();
        let mempool_retriever =
            MempoolRetriever::new(clients.rpc.clone(), mempool_snapshot.clone());
        let transaction_retriever = TransactionRpcRetriever::new(clients.rpc.clone());
        let cluster_retriever = ClusterRpcRetriever::new(clients.rpc.clone());
        let block_retriever = BlockRpcRetriever::new(clients.rpc.clone());

        Self {
            repos,
            pubsub,
            mempool_snapshot,
            cluster_snapshot,
            mempool_retriever,
            transaction_retriever,
            cluster_retriever,
            block_retriever,
            readiness: Readiness::default(),
        }
    }

    pub fn app_state(&self) -> AppState {
        AppState {
            readiness: self.readiness.clone(),
            mempool_retriever: self.mempool_retriever.clone(),
            mempool_service: self.mempool_service(),
            block_service: self.block_service(),
            cluster_service: self.cluster_service(),
            home_service: self.home_service(),
            snapshot_service: self.snapshot_service(),
            bootstrap_service: self.bootstrap_service(),
        }
    }

    pub fn bootstrap_service(&self) -> BootstrapService {
        BootstrapService::new(
            self.repos.mempool_delta.clone(),
            self.mempool_retriever.clone(),
            self.mempool_service(),
            self.block_service(),
            self.cluster_service(),
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
            self.repos.transaction.clone(),
            self.repos.cluster_membership.clone(),
            self.cluster_retriever.clone(),
            self.cluster_delta_service(),
        )
    }

    pub fn block_service(&self) -> BlockService<BR, CR> {
        BlockService::new(
            self.repos.block.clone(),
            self.repos.transaction.clone(),
            self.repos.mempool_delta.clone(),
            self.cluster_service(),
            self.block_retriever.clone(),
            self.pubsub.clone(),
        )
    }

    pub fn mempool_service(&self) -> MempoolService<TR, CR> {
        MempoolService::new(
            self.repos.mempool_delta.clone(),
            self.repos.transaction.clone(),
            self.transaction_retriever.clone(),
            self.cluster_service(),
            self.pubsub.clone(),
        )
    }

    pub fn home_service(&self) -> HomeService<CR> {
        HomeService::new(
            self.repos.block.clone(),
            self.repos.mempool_delta.clone(),
            self.mempool_retriever.clone(),
            self.cluster_service(),
            self.pubsub.clone(),
        )
    }

    pub fn snapshot_service(&self) -> SnapshotService {
        SnapshotService::new(
            self.repos.snapshot.clone(),
            self.cluster_snapshot.clone(),
            self.mempool_snapshot.clone(),
        )
    }

    /// Swaps the transaction retriever, e.g. for a mock in tests.
    pub fn with_transaction_retriever<T2: TransactionRetriever>(self, r: T2) -> Deps<T2, CR, BR> {
        Deps {
            repos: self.repos,
            pubsub: self.pubsub,
            mempool_snapshot: self.mempool_snapshot,
            cluster_snapshot: self.cluster_snapshot,
            mempool_retriever: self.mempool_retriever,
            transaction_retriever: r,
            cluster_retriever: self.cluster_retriever,
            block_retriever: self.block_retriever,
            readiness: self.readiness,
        }
    }

    /// Swaps the cluster retriever, e.g. for a mock in tests.
    pub fn with_cluster_retriever<C2: ClusterRetriever>(self, r: C2) -> Deps<TR, C2, BR> {
        Deps {
            repos: self.repos,
            pubsub: self.pubsub,
            mempool_snapshot: self.mempool_snapshot,
            cluster_snapshot: self.cluster_snapshot,
            mempool_retriever: self.mempool_retriever,
            transaction_retriever: self.transaction_retriever,
            cluster_retriever: r,
            block_retriever: self.block_retriever,
            readiness: self.readiness,
        }
    }

    /// Swaps the block retriever, e.g. for a mock in tests.
    pub fn with_block_retriever<B2: BlockRetriever>(self, r: B2) -> Deps<TR, CR, B2> {
        Deps {
            repos: self.repos,
            pubsub: self.pubsub,
            mempool_snapshot: self.mempool_snapshot,
            cluster_snapshot: self.cluster_snapshot,
            mempool_retriever: self.mempool_retriever,
            transaction_retriever: self.transaction_retriever,
            cluster_retriever: self.cluster_retriever,
            block_retriever: r,
            readiness: self.readiness,
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
}
