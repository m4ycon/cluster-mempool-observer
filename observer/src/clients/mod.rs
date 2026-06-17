pub mod rpc_client;

use rpc_client::RpcClient;
use shared::pubsub::PubSub;

#[derive(Clone)]
pub struct Clients {
    pub pubsub: PubSub,
    pub rpc: RpcClient,
}
