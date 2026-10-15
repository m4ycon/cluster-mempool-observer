use jsonrpc::simple_http::{Builder, SimpleHttpTransport};
use jsonrpc::{Request, Response, Transport};
use std::fmt;
use std::sync::Mutex;
use std::time::Duration;

/// A JSON-RPC transport that keeps its HTTP connections open between calls.
///
/// Each `SimpleHttpTransport` holds one keep-alive socket and serializes the
/// calls made through it, so a request takes an idle one from the pool, or
/// builds a new one when every connection is busy, and puts it back afterwards.
/// The pool therefore grows to the peak number of concurrent calls and no
/// further.
///
/// On a stale socket, `SimpleHttpTransport` reconnects and sends the request
/// again, so a call can reach the node twice. That is only safe because every
/// call the observer makes is a read.
pub struct PooledJsonRpcTransport {
    url: String,
    user: String,
    pass: String,
    timeout: Duration,
    idle: Mutex<Vec<SimpleHttpTransport>>,
}

impl PooledJsonRpcTransport {
    pub fn new(url: String, user: String, pass: String, timeout: Duration) -> Self {
        Self {
            url,
            user,
            pass,
            timeout,
            idle: Mutex::new(Vec::new()),
        }
    }

    fn checkout(&self) -> Result<SimpleHttpTransport, jsonrpc::Error> {
        if let Some(transport) = self.idle.lock().expect("poisoned pool").pop() {
            return Ok(transport);
        }

        // Never cloned from a template, since clones share one socket.
        Ok(Builder::new()
            .url(&self.url)?
            .timeout(self.timeout)
            .auth(&self.user, Some(&self.pass))
            .build())
    }

    fn checkin(&self, transport: SimpleHttpTransport) {
        self.idle.lock().expect("poisoned pool").push(transport);
    }
}

impl Transport for PooledJsonRpcTransport {
    fn send_request(&self, req: Request) -> Result<Response, jsonrpc::Error> {
        let transport = self.checkout()?;
        let response = transport.send_request(req);
        self.checkin(transport);
        response
    }

    fn send_batch(&self, reqs: &[Request]) -> Result<Vec<Response>, jsonrpc::Error> {
        let transport = self.checkout()?;
        let responses = transport.send_batch(reqs);
        self.checkin(transport);
        responses
    }

    fn fmt_target(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.url)
    }
}
