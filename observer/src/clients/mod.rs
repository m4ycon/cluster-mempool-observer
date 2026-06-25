pub mod rpc_client;
pub mod zmq_client;

use rpc_client::RpcClient;
use shared::pubsub::PubSub;
use zmq_client::ZmqClient;

#[derive(Clone)]
pub struct Clients {
    pub pubsub: PubSub,
    pub rpc: RpcClient,
    pub zmq: ZmqClient,
}
