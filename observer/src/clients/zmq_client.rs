use crate::error::ObserverError;
use crate::infra::config::ZmqConfig;
use bitcoincore_zmq::{MessageStream, subscribe_async};

#[derive(Clone)]
pub struct ZmqClient {
    blocks_endpoint: String,
}

impl ZmqClient {
    pub fn new(config: &ZmqConfig) -> Self {
        Self {
            blocks_endpoint: config.blocks_endpoint.clone(),
        }
    }

    /// Subscribes to the `hashblock` stream
    pub fn blocks(&self) -> Result<MessageStream, ObserverError> {
        let endpoint = self.blocks_endpoint.as_str();
        tracing::info!("Subscribing to ZMQ blocks at {endpoint}");
        subscribe_async(&[endpoint]).map_err(|e| ObserverError::FailedToConnect(e.to_string()))
    }
}
