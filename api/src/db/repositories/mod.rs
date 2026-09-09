mod block;
mod cluster;
mod cluster_membership;
mod mempool_admission;
mod mempool_delta;
mod mempool_ledger;
mod snapshot;
mod system_event;
mod transaction;

pub use block::BlockRepository;
pub use cluster::ClusterRepository;
pub use cluster_membership::{ClusterMembershipRepository, ClusterMembershipUpdate};
pub use mempool_admission::MempoolAdmissionRepository;
pub use mempool_delta::MempoolDeltaRepository;
pub use mempool_ledger::{FlushOutcome, MempoolLedgerRepository};
pub use snapshot::{NATIVE_RESOLUTION_SECS, SnapshotRepository};
pub use system_event::SystemEventRepository;
pub use transaction::TransactionRepository;

use crate::db::pool::DbPool;
use diesel_async::pooled_connection::deadpool::PoolError;

/// Postgres caps a statement at 65535 bind params; `NewTransaction` has 9 columns.
pub const TRANSACTION_INSERT_CHUNK_SIZE: usize = 1000;

/// Avoid inserting too many rows at once, which can cause performance issues or exceed database limits.
pub(super) const MEMPOOL_DELTA_INSERT_CHUNK_SIZE: usize = 10_000;

#[derive(Clone)]
pub struct Repos {
    pub block: BlockRepository,
    pub cluster: ClusterRepository,
    pub cluster_membership: ClusterMembershipRepository,
    pub mempool_admission: MempoolAdmissionRepository,
    pub mempool_delta: MempoolDeltaRepository,
    pub mempool_ledger: MempoolLedgerRepository,
    pub snapshot: SnapshotRepository,
    pub system_event: SystemEventRepository,
    pub transaction: TransactionRepository,
}

impl Repos {
    pub fn new(pool: DbPool) -> Self {
        Self {
            block: BlockRepository::new(pool.clone()),
            cluster: ClusterRepository::new(pool.clone()),
            cluster_membership: ClusterMembershipRepository::new(pool.clone()),
            mempool_admission: MempoolAdmissionRepository::new(pool.clone()),
            mempool_delta: MempoolDeltaRepository::new(pool.clone()),
            mempool_ledger: MempoolLedgerRepository::new(pool.clone()),
            snapshot: SnapshotRepository::new(pool.clone()),
            system_event: SystemEventRepository::new(pool.clone()),
            transaction: TransactionRepository::new(pool),
        }
    }
}

/// Error from a repository call: either checking out a pooled connection or
/// running the query.
#[derive(Debug)]
pub enum RepoError {
    Pool(PoolError),
    Query(diesel::result::Error),
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoError::Pool(e) => write!(f, "connection pool error: {e}"),
            RepoError::Query(e) => write!(f, "query error: {e}"),
        }
    }
}

impl std::error::Error for RepoError {}

impl From<PoolError> for RepoError {
    fn from(e: PoolError) -> Self {
        RepoError::Pool(e)
    }
}

impl From<diesel::result::Error> for RepoError {
    fn from(e: diesel::result::Error) -> Self {
        RepoError::Query(e)
    }
}

pub type RepoResult<T> = Result<T, RepoError>;
