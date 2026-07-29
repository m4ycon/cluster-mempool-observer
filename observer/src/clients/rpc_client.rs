use crate::error::ObserverError;
use crate::infra::config::RpcConfig;
use corepc_client::client_sync::{Auth, v31::Client};
use shared::metrics::record_elapsed;
use std::sync::Arc;
use std::time::Instant;

/// Round trip of one RPC call, labelled by the node method it invokes.
const RPC_CALL_SECONDS: &str = "rpc_call_seconds";

/// Calls that failed, whether the node rejected them or the blocking task died.
const RPC_CALL_ERRORS_TOTAL: &str = "rpc_call_errors_total";

#[derive(Clone)]
pub struct RpcClient {
    client: Arc<Client>,
}

impl RpcClient {
    pub fn new(config: &RpcConfig) -> Result<Self, ObserverError> {
        let auth = Auth::UserPass(config.user.to_string(), config.pass.to_string());
        let client = Client::new_with_auth(&format!("http://{}", config.host), auth)
            .map_err(|e| ObserverError::FailedToConnect(e.to_string()))?;
        Ok(Self {
            client: Arc::new(client),
        })
    }

    pub async fn call<F, T>(&self, method: &'static str, rpc_call: F) -> Result<T, ObserverError>
    where
        F: FnOnce(&Client) -> Result<T, corepc_client::client_sync::Error> + Send + 'static,
        T: Send + 'static,
    {
        let client = Arc::clone(&self.client);
        let started = Instant::now();
        let joined = tokio::task::spawn_blocking(move || rpc_call(&client)).await;

        let result = match joined {
            Ok(response) => response.map_err(|e| ObserverError::FailedToFetch(e.to_string())),
            Err(e) => Err(ObserverError::FailedToFetch(e.to_string())),
        };

        record_elapsed(RPC_CALL_SECONDS, &[("method", method)], started);
        if result.is_err() {
            metrics::counter!(RPC_CALL_ERRORS_TOTAL, "method" => method).increment(1);
        }
        result
    }
}
