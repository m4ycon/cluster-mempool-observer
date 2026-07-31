pub mod rpc_client;
pub mod zmq_client;

use crate::error::ObserverError;
use crate::infra::config::Config;
use rpc_client::RpcClient;
use shared::pubsub::PubSub;
use zmq_client::ZmqClient;

#[derive(Clone)]
pub struct Clients {
    pub pubsub: PubSub,
    pub rpc: RpcClient,
    pub zmq: ZmqClient,
}

impl Clients {
    pub fn new(config: &Config) -> Result<Self, ObserverError> {
        Ok(Self {
            pubsub: PubSub::new(),
            rpc: RpcClient::new(&config.rpc)?,
            zmq: ZmqClient::new(&config.zmq),
        })
    }
}
