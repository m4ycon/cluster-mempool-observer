use crate::error::ObserverError;
use crate::infra::config::RpcConfig;
use corepc_client::client_sync::{Auth, v31::Client};
use std::sync::Arc;

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

    pub async fn call<F, T>(&self, rpc_call: F) -> Result<T, ObserverError>
    where
        F: FnOnce(&Client) -> Result<T, corepc_client::client_sync::Error> + Send + 'static,
        T: Send + 'static,
    {
        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || rpc_call(&client))
            .await
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))?
            .map_err(|e| ObserverError::FailedToFetch(e.to_string()))
    }
}
