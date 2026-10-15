//! A Bitcoin Core client over [`PooledJsonRpcTransport`].
//!
//! corepc's own `Client` always sends through a transport that opens a TCP
//! connection per call and cannot be given another. Its per-method macros only
//! need a type named `Client` with a `call` method, so this module defines one
//! and invokes the macros for the RPCs the observer makes. Each macro body
//! names its types unqualified, which is why the imports below look unused.

use super::pooled_json_rpc_transport::PooledJsonRpcTransport;
use corepc_client::bitcoin::{self, Block, BlockHash, Txid};
use corepc_client::client_sync::{Error, Result};
use corepc_client::types::v31::*;
use std::time::Duration;

/// Matches the timeout corepc gives its own client.
const TIMEOUT: Duration = Duration::from_secs(60);

pub struct Client {
    inner: jsonrpc::Client,
}

impl Client {
    pub fn new(url: &str, user: &str, pass: &str) -> Self {
        let transport = PooledJsonRpcTransport::new(url.into(), user.into(), pass.into(), TIMEOUT);
        Self {
            inner: jsonrpc::Client::with_transport(transport),
        }
    }

    pub fn call<T: for<'a> serde::de::Deserialize<'a>>(
        &self,
        method: &str,
        args: &[serde_json::Value],
    ) -> Result<T> {
        let raw = serde_json::value::to_raw_value(args)?;
        let req = self.inner.build_request(method, Some(&*raw));
        let resp = self.inner.send_request(req).map_err(Error::from)?;
        Ok(resp.result()?)
    }
}

/// corepc's own helper of this name is crate-private, and the macros call it
/// unqualified.
fn into_json<T: serde::Serialize>(
    val: T,
) -> std::result::Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(val)
}

corepc_client::impl_client_v17__get_block_count!();
corepc_client::impl_client_v17__get_block_hash!();
corepc_client::impl_client_v29__get_block!();
corepc_client::impl_client_v17__get_blockchain_info!();
corepc_client::impl_client_v31__get_mempool_cluster!();
corepc_client::impl_client_v17__get_mempool_entry!();
corepc_client::impl_client_v31__get_mempool_feerate_diagram!();
corepc_client::impl_client_v17__get_mempool_info!();
corepc_client::impl_client_v17__get_network_info!();
corepc_client::impl_client_v17__get_raw_mempool!();
corepc_client::impl_client_v17__get_raw_transaction!();
