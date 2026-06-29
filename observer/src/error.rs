use std::fmt::Display;

#[derive(Debug)]
pub enum ObserverError {
    FailedToConnect(String),
    FailedToFetch(String),
    InvalidParams(String),
    InvalidZmqMessage(String),
    TxNotFoundInMempool(String),
    Other(String),
}

impl Display for ObserverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObserverError::FailedToConnect(msg) => write!(f, "Failed to connect: {}", msg),
            ObserverError::FailedToFetch(msg) => write!(f, "Failed to fetch: {}", msg),
            ObserverError::InvalidParams(msg) => write!(f, "Invalid parameters: {}", msg),
            ObserverError::InvalidZmqMessage(msg) => write!(f, "Invalid ZMQ message: {}", msg),
            ObserverError::TxNotFoundInMempool(msg) => {
                write!(f, "Transaction not found in mempool: {}", msg)
            }
            ObserverError::Other(msg) => write!(f, "Other error: {}", msg),
        }
    }
}

impl From<std::io::Error> for ObserverError {
    fn from(err: std::io::Error) -> Self {
        ObserverError::Other(err.to_string())
    }
}
