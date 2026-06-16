#[derive(Debug)]
pub enum ObserverError {
    FailedToConnect(String),
    FailedToFetch(String),
    InvalidParams(String),
    Other(String),
}

impl From<std::io::Error> for ObserverError {
    fn from(err: std::io::Error) -> Self {
        ObserverError::Other(err.to_string())
    }
}
