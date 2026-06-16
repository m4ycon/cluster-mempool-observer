pub mod rpc_client;

use async_nats::Client;
use rpc_client::RpcClient;

#[derive(Clone)]
pub struct Clients {
    pub nats: Client,
    pub rpc: RpcClient,
}
