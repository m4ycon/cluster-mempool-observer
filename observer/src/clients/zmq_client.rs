use crate::error::ObserverError;
use crate::infra::config::ZmqConfig;
use bitcoincore_zmq::{
    subscribe_async_monitor_stream::MessageStream, subscribe_async_wait_handshake,
};
use std::time::Duration;
use tokio::time::timeout;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

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

    /// Subscribes to the `hashblock` stream and waits for the socket handshake.
    pub async fn blocks(&self) -> Result<MessageStream, ObserverError> {
        let endpoint = self.blocks_endpoint.as_str();
        tracing::info!("Subscribing to ZMQ blocks at {endpoint}");

        match timeout(
            HANDSHAKE_TIMEOUT,
            subscribe_async_wait_handshake(&[endpoint]),
        )
        .await
        {
            Ok(Ok(stream)) => Ok(stream),
            Ok(Err(e)) => Err(ObserverError::FailedToConnect(format!(
                "zmq subscribe to {endpoint} failed: {e}"
            ))),
            Err(_) => Err(ObserverError::FailedToConnect(format!(
                "no zmq handshake from {endpoint} within {HANDSHAKE_TIMEOUT:?} -- check the host \
                 and port, and that bitcoind publishes hashblock there"
            ))),
        }
    }
}

#[cfg(test)]
mod blocks_tests {
    use super::*;

    /// `MessageStream` has no `Debug`, so `expect_err` is out.
    fn connect_failure(result: Result<MessageStream, ObserverError>) -> String {
        match result {
            Ok(_) => panic!("a dead endpoint must not produce a stream"),
            Err(ObserverError::FailedToConnect(msg)) => msg,
            Err(other) => panic!("expected FailedToConnect, got {other}"),
        }
    }

    fn client_for(endpoint: &str) -> ZmqClient {
        ZmqClient::new(&ZmqConfig {
            blocks_endpoint: endpoint.into(),
        })
    }

    #[tokio::test]
    async fn a_closed_port_fails_instead_of_yielding_a_silent_stream() {
        let msg = connect_failure(client_for("tcp://127.0.0.1:1").blocks().await);
        assert!(msg.contains("no zmq handshake"), "got: {msg}");
    }

    #[tokio::test]
    async fn a_peer_that_does_not_speak_zmtp_fails_too() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("read local addr").port();

        let msg = connect_failure(
            client_for(&format!("tcp://127.0.0.1:{port}"))
                .blocks()
                .await,
        );
        assert!(msg.contains("no zmq handshake"), "got: {msg}");
    }

    #[tokio::test]
    async fn a_malformed_endpoint_reports_the_zmq_error() {
        let msg = connect_failure(client_for("not-a-transport://nope").blocks().await);
        assert!(msg.contains("zmq subscribe to"), "got: {msg}");
    }
}
