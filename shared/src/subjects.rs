use std::fmt;

/// Subjects that events are published to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Subject {
    MempoolDelta,
    BlockConnected,
}

impl Subject {
    /// All subjects, used to pre-create a channel per subject on the bus.
    pub const ALL: [Subject; 2] = [Subject::MempoolDelta, Subject::BlockConnected];

    pub fn as_str(&self) -> &'static str {
        match self {
            Subject::MempoolDelta => "rpc.mempooldelta",
            Subject::BlockConnected => "zmq.blockconnected",
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
