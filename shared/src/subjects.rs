use std::fmt;

/// Subjects that events are published to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    RawMempool,
    RawTransaction,
    RequestRawTransaction,
}

impl Subject {
    pub fn as_str(&self) -> &'static str {
        match self {
            Subject::RawMempool => "rpc.rawmempool",
            Subject::RawTransaction => "rpc.rawtransaction",
            Subject::RequestRawTransaction => "request.rawtransaction",
        }
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for Subject {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
