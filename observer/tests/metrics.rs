use corepc_client::bitcoin::hashes::Hash;
use corepc_client::bitcoin::{BlockHash, Txid};
use observer::clients::zmq_client::ZmqClient;
use observer::error::ObserverError;
use observer::infra::config::{WatchersConfig, ZmqConfig};
use observer::publisher::publish_event;
use observer::watchers::block::BlockWatcher;
use observer::watchers::watcher_trait::{Watcher, WatcherRPC, WatcherZMQ};
use serde::Serialize;
use shared::pubsub::PubSub;
use shared::subjects::Subject;
use std::collections::HashMap;
use testkit::deps::inert_rpc;
use testkit::metrics::{assert_no_series, assert_series, capture};

fn block_watcher() -> BlockWatcher {
    BlockWatcher::new(ZmqClient::new(&ZmqConfig {
        blocks_endpoint: None,
    }))
}

// region: rpc_call_seconds / rpc_call_errors_total

#[test]
fn rpc_call_records_its_duration_under_the_method_label() {
    let rendered = capture(async {
        let _ = inert_rpc()
            .call("getblockcount", |client| client.get_block_count())
            .await;
    });
    assert_series(
        &rendered,
        r#"rpc_call_seconds_count{method="getblockcount"} 1"#,
    );
}

/// A failed call is still timed: the time was spent, and a node that is slow to
/// reject is exactly what the histogram should show.
#[test]
fn rpc_call_counts_and_times_failures() {
    let rendered = capture(async {
        let _ = inert_rpc()
            .call("getblockcount", |client| client.get_block_count())
            .await;
    });
    assert_series(
        &rendered,
        r#"rpc_call_errors_total{method="getblockcount"} 1"#,
    );
    assert_series(
        &rendered,
        r#"rpc_call_seconds_count{method="getblockcount"} 1"#,
    );
}

/// Distinct methods must not share a series, otherwise a slow `getblock` would
/// be hidden by a fast `getblockcount`.
#[test]
fn rpc_call_separates_methods() {
    let rendered = capture(async {
        let rpc = inert_rpc();
        let _ = rpc
            .call("getblockcount", |client| client.get_block_count())
            .await;
        let _ = rpc
            .call("getrawmempool", |client| client.get_raw_mempool())
            .await;
    });
    assert_series(
        &rendered,
        r#"rpc_call_seconds_count{method="getblockcount"} 1"#,
    );
    assert_series(
        &rendered,
        r#"rpc_call_seconds_count{method="getrawmempool"} 1"#,
    );
}

// endregion

// region: watcher_poll_seconds / watcher_poll_errors_total

/// What one poll of the source returned.
enum Outcome {
    Changed,
    Unchanged,
    Failed,
}

struct FakeWatcher {
    outcome: Outcome,
}

impl Watcher for FakeWatcher {
    type Event = &'static str;

    fn get_publish_subject(&self) -> Subject {
        Subject::MempoolDelta
    }

    fn is_enabled(&self, _config: &WatchersConfig) -> bool {
        true
    }
}

impl WatcherRPC for FakeWatcher {
    type Response = ();

    async fn watch(&mut self) -> Result<Option<Self::Response>, ObserverError> {
        match self.outcome {
            Outcome::Changed => Ok(Some(())),
            Outcome::Unchanged => Ok(None),
            Outcome::Failed => Err(ObserverError::FailedToFetch("node is down".into())),
        }
    }

    fn get_watch_rate(&self) -> u32 {
        1
    }

    fn to_event(&self, _response: &Self::Response) -> Self::Event {
        "event"
    }
}

fn poll(outcome: Outcome) -> String {
    capture(async move {
        FakeWatcher { outcome }.poll_once(&PubSub::new()).await;
    })
}

#[test]
fn poll_seconds_records_a_poll_that_published() {
    let rendered = poll(Outcome::Changed);
    assert_series(
        &rendered,
        r#"watcher_poll_seconds_count{subject="rpc.mempooldelta"} 1"#,
    );
    assert_series(
        &rendered,
        r#"pubsub_published_total{subject="rpc.mempooldelta"} 1"#,
    );
}

/// An unchanged poll still did the work of asking, so it is timed -- but it
/// must not look like a publish.
#[test]
fn poll_seconds_records_a_poll_with_no_change() {
    let rendered = poll(Outcome::Unchanged);
    assert_series(
        &rendered,
        r#"watcher_poll_seconds_count{subject="rpc.mempooldelta"} 1"#,
    );
    assert_no_series(&rendered, "pubsub_published_total");
}

/// The loop swallows the error and keeps polling, so without this counter a
/// node that stopped answering is indistinguishable from an idle mempool.
#[test]
fn poll_errors_total_counts_a_failed_poll() {
    let rendered = poll(Outcome::Failed);
    assert_series(
        &rendered,
        r#"watcher_poll_errors_total{subject="rpc.mempooldelta"} 1"#,
    );
    assert_series(
        &rendered,
        r#"watcher_poll_seconds_count{subject="rpc.mempooldelta"} 1"#,
    );
}

#[test]
fn poll_errors_total_stays_absent_on_a_healthy_poll() {
    assert_no_series(&poll(Outcome::Changed), "watcher_poll_errors_total");
}

// endregion

// region: zmq_messages_total / zmq_message_handle_seconds / zmq_errors_total

#[test]
fn zmq_records_a_handled_message() {
    let rendered = capture(async {
        block_watcher()
            .handle_one(
                bitcoincore_zmq::Message::HashBlock(BlockHash::from_byte_array([7u8; 32]), 0),
                &PubSub::new(),
            )
            .await;
    });

    assert_series(
        &rendered,
        r#"zmq_messages_total{subject="zmq.blockconnected"} 1"#,
    );
    assert_series(
        &rendered,
        r#"zmq_message_handle_seconds_count{subject="zmq.blockconnected"} 1"#,
    );
    assert_series(
        &rendered,
        r#"pubsub_published_total{subject="zmq.blockconnected"} 1"#,
    );
    assert_no_series(&rendered, "zmq_errors_total");
}

/// A message of the wrong type is counted as received and as an error, but
/// never as handled -- it produced no event.
#[test]
fn zmq_errors_total_counts_an_unexpected_message() {
    let rendered = capture(async {
        block_watcher()
            .handle_one(
                bitcoincore_zmq::Message::HashTx(Txid::from_byte_array([7u8; 32]), 0),
                &PubSub::new(),
            )
            .await;
    });

    assert_series(
        &rendered,
        r#"zmq_errors_total{subject="zmq.blockconnected",kind="handle"} 1"#,
    );
    assert_series(
        &rendered,
        r#"zmq_messages_total{subject="zmq.blockconnected"} 1"#,
    );
    assert_no_series(&rendered, "zmq_message_handle_seconds");
    assert_no_series(&rendered, "pubsub_published_total");
}

// endregion

// region: event_serialize_errors_total

/// Non-string map keys are not representable in JSON, so serialization fails.
#[derive(Debug, Serialize)]
struct Unserializable {
    keyed_by_tuple: HashMap<(u8, u8), u8>,
}

/// The event is dropped before it reaches the bus, so nothing downstream can
/// report it.
#[test]
fn event_serialize_errors_total_counts_a_dropped_event() {
    let rendered = capture(async {
        let event = Unserializable {
            keyed_by_tuple: HashMap::from([((1, 2), 3)]),
        };
        publish_event(&PubSub::new(), Subject::BlockConnected, &event).await;
    });

    assert_series(
        &rendered,
        r#"event_serialize_errors_total{subject="zmq.blockconnected"} 1"#,
    );
    assert_no_series(&rendered, "pubsub_published_total");
}

// endregion
