use crate::config::{INERT_ZMQ_ENDPOINT, get_config_with_rpc_config};
use crate::mocks::{MockBlockRetriever, MockClusterRetriever};
use crate::postgres::{inert_pool, isolated_pool};
use api::db::{DbPool, Repos};
use api::infra::deps::Deps;
use api::infra::readiness::Phase;
use api::services::block::BlockService;
use api::services::cluster::ClusterService;
use api::services::snapshot::SnapshotService;
use corepc_node::Node;
use observer::clients::Clients;
use observer::clients::rpc_client::RpcClient;
use observer::infra::config::{Config, RpcConfig, ZmqConfig};
use shared::events::ClusterRef;
use shared::models::{GetBlockModel, GetMempoolClusterModel};
use std::collections::HashSet;

/// Config aimed at a closed port. Nothing connects at build time, so this is
/// safe anywhere; every call made through it fails fast.
pub fn inert_config() -> Config {
    Config {
        rpc: RpcConfig {
            host: "127.0.0.1:1".into(),
            user: String::new(),
            pass: String::new(),
        },
        zmq: ZmqConfig {
            blocks_endpoint: INERT_ZMQ_ENDPOINT.into(),
        },
        ..Config::default()
    }
}

/// An RPC client that reaches nothing.
pub fn inert_rpc() -> RpcClient {
    RpcClient::new(&inert_config().rpc).expect("rpc client builds without connecting")
}

/// Marks a container as past startup, so the router's readiness gate lets data
/// routes through. Tests stand in for an api that has already bootstrapped.
fn ready(deps: Deps) -> Deps {
    deps.readiness.set_phase(Phase::Ready);
    deps
}

/// Node clients that reach nothing: fresh pubsub, inert rpc, default zmq.
pub fn inert_clients() -> Clients {
    Clients::new(&inert_config()).expect("clients build without connecting")
}

/// Node clients wired to a running regtest bitcoind.
pub fn clients_for_node(node: &Node) -> Clients {
    Clients::new(&get_config_with_rpc_config(node)).expect("init node clients")
}

/// A container over the given pool with no reachable node. Swap in mocks with
/// `Deps::with_*` for whatever the test actually exercises.
pub fn deps(pool: DbPool) -> Deps {
    ready(Deps::new(Repos::new(pool), &inert_clients()))
}

/// A container over the given pool, wired to a live regtest node.
pub fn deps_for_node(pool: DbPool, node: &Node) -> Deps {
    ready(Deps::new(Repos::new(pool), &clients_for_node(node)))
}

/// A container over an isolated (rolled-back) test pool and no reachable node.
pub async fn isolated_deps() -> Deps {
    deps(isolated_pool().await)
}

/// A container over an isolated test pool, wired to a live regtest node.
pub async fn isolated_deps_for_node(node: &Node) -> Deps {
    deps_for_node(isolated_pool().await, node)
}

/// The container over an inert pool and no reachable node -- for tests that only
/// care that instrumentation fires, not that calls succeed.
pub fn inert_deps() -> Deps {
    deps(inert_pool())
}

/// A cluster service over the given pool, answering cluster lookups from a
/// fixed set instead of the node.
pub fn cluster_service(
    pool: DbPool,
    clusters: Vec<GetMempoolClusterModel>,
) -> ClusterService<MockClusterRetriever> {
    deps(pool)
        .with_cluster_retriever(MockClusterRetriever::with_clusters(clusters))
        .cluster_service()
}

/// Like [`cluster_service`], but a txid with no fixture is reported as gone from
/// the mempool instead of as a cluster of its own.
pub fn strict_cluster_service(
    pool: DbPool,
    clusters: Vec<GetMempoolClusterModel>,
) -> ClusterService<MockClusterRetriever> {
    deps(pool)
        .with_cluster_retriever(MockClusterRetriever::strict(clusters))
        .cluster_service()
}

/// A snapshot service over the given pool, sampling the given in-memory state
/// instead of whatever a running api would have accumulated.
pub fn snapshot_service(
    pool: DbPool,
    clusters: Vec<ClusterRef>,
    mempool_txids: HashSet<String>,
) -> SnapshotService {
    let deps = deps(pool);
    deps.cluster_snapshot.seed(clusters);
    deps.mempool_ledger.seed(mempool_txids);
    deps.snapshot_service()
}

/// A block service over the given pool, answering both block and cluster
/// lookups from fixed sets instead of the node.
pub fn block_service(
    pool: DbPool,
    blocks: Vec<GetBlockModel>,
    clusters: Vec<GetMempoolClusterModel>,
) -> BlockService<MockBlockRetriever, MockClusterRetriever> {
    deps(pool)
        .with_cluster_retriever(MockClusterRetriever::with_clusters(clusters))
        .with_block_retriever(MockBlockRetriever::with_blocks(blocks))
        .block_service()
}
