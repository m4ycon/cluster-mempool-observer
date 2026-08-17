mod block;
mod cluster;
mod cluster_membership;
mod mempool_delta;
mod snapshot;
mod system_event;
mod transaction;

pub use block::BlockRepository;
pub use cluster::ClusterRepository;
pub use cluster_membership::{ClusterMembershipRepository, ClusterMembershipUpdate};
pub use mempool_delta::MempoolDeltaRepository;
pub use snapshot::{NATIVE_RESOLUTION_SECS, SnapshotRepository};
pub use system_event::SystemEventRepository;
pub use transaction::TransactionRepository;

use crate::db::pool::DbPool;
use diesel_async::pooled_connection::deadpool::PoolError;

#[derive(Clone)]
pub struct Repos {
    pub block: BlockRepository,
    pub cluster: ClusterRepository,
    pub cluster_membership: ClusterMembershipRepository,
    pub mempool_delta: MempoolDeltaRepository,
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
            mempool_delta: MempoolDeltaRepository::new(pool.clone()),
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
