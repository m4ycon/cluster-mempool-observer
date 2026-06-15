use crate::error::ObserverError;
use crate::infra::config::RpcConfig;
use corepc_client::client_sync::{Auth, v31::Client};
use std::sync::{Arc, OnceLock};

// works like a singleton, with `init` and `get`
static RPC_CLIENT: OnceLock<Arc<RpcClient>> = OnceLock::new();

pub fn init(config: &RpcConfig) -> Result<(), ObserverError> {
    let client = RpcClient::new(&config.host, &config.user, &config.pass)?;
    let _ = RPC_CLIENT.set(Arc::new(client));
    Ok(())
}

pub fn get() -> &'static Arc<RpcClient> {
    RPC_CLIENT.get().expect("RPC client not initialized")
}

pub struct RpcClient {
    client: Arc<Client>,
}

impl RpcClient {
    pub fn new(rpc_host: &str, rpc_user: &str, rpc_pass: &str) -> Result<Self, ObserverError> {
        let auth = Auth::UserPass(rpc_user.to_string(), rpc_pass.to_string());
        let client = Client::new_with_auth(&format!("http://{}", rpc_host), auth)
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
