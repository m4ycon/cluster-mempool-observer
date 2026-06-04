use std::sync::{Arc, LazyLock};

use corepc_client::client_sync::{Auth, v31::Client};

use crate::extractors::extractor_trait::ExtractorError;

pub static RPC_CLIENT: LazyLock<Arc<RpcClient>> = LazyLock::new(|| {
    let rpc_host = "127.0.0.1:8332";
    let rpc_user = "fake-user";
    let rpc_pass = "fake-pass";
    let client = RpcClient::new(rpc_host, rpc_user, rpc_pass).expect("Failed to create RPC client");
    Arc::new(client)
});

pub struct RpcClient {
    client: Arc<Client>,
}

impl RpcClient {
    pub fn new(rpc_host: &str, rpc_user: &str, rpc_pass: &str) -> Result<Self, ExtractorError> {
        let auth = Auth::UserPass(rpc_user.to_string(), rpc_pass.to_string());
        let client = Client::new_with_auth(&format!("http://{}", rpc_host), auth)
            .map_err(|e| ExtractorError::FailedToConnect(e.to_string()))?;
        Ok(Self {
            client: Arc::new(client),
        })
    }

    pub async fn call<F, T>(&self, rpc_call: F) -> Result<T, ExtractorError>
    where
        F: FnOnce(&Client) -> Result<T, corepc_client::client_sync::Error> + Send + 'static,
        T: Send + 'static,
    {
        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || rpc_call(&client))
            .await
            .map_err(|e| ExtractorError::FailedToExtract(e.to_string()))?
            .map_err(|e| ExtractorError::FailedToExtract(e.to_string()))
    }
}
