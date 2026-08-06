use crate::clients::zmq_client::ZmqClient;
use crate::error::ObserverError;
use crate::watchers::watcher_trait::{Watcher, WatcherZMQ};
use bitcoincore_zmq::Message;
use shared::events::BlockConnectedEvent;
use shared::subjects::Subject;

pub struct BlockWatcher {
    zmq: ZmqClient,
}

impl BlockWatcher {
    pub fn new(zmq: ZmqClient) -> Self {
        Self { zmq }
    }
}

impl Watcher for BlockWatcher {
    type Event = BlockConnectedEvent;

    fn get_publish_subject(&self) -> Subject {
        Subject::BlockConnected
    }
}

impl WatcherZMQ for BlockWatcher {
    fn get_stream(&self) -> Result<bitcoincore_zmq::MessageStream, ObserverError> {
        self.zmq.blocks()
    }

    fn handle_message(&self, msg: bitcoincore_zmq::Message) -> Result<Self::Event, ObserverError> {
        match msg {
            Message::HashBlock(hash, _seq) => Ok(BlockConnectedEvent {
                hash: hash.to_string(),
            }),
            _ => Err(ObserverError::InvalidZmqMessage(format!(
                "unexpected ZMQ message type: {msg:?}"
            ))),
        }
    }
}
