use api::infra::deps::Deps;
use api::infra::router;
use futures::{SinkExt, StreamExt};
use shared::events::{ClusterDeltaEvent, ClusterRef};
use shared::subjects::Subject;
use shared::ws::{ClientFrame, ServerEvent, WsSubject};
use std::time::Duration;
use testkit::deps::inert_deps;
use testkit::fixtures::{ClusterRefFixture, MempoolDeltaEventFixture};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const READ_TIMEOUT: Duration = Duration::from_secs(2);
const ABSENCE_TIMEOUT: Duration = Duration::from_millis(300);

/// Binds the real router on a real socket and returns the port it is
/// listening on.
async fn spawn_server(deps: &Deps) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();
    let app = router::build(deps.app_state());
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server error");
    });
    port
}

async fn connect(port: u16) -> Socket {
    let (socket, _response) = connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .expect("client connects");
    socket
}

async fn send_frame(socket: &mut Socket, frame: &ClientFrame) {
    let text = serde_json::to_string(frame).expect("frame serializes");
    socket.send(Message::text(text)).await.expect("frame sends");
}

async fn subscribe(socket: &mut Socket, subject: WsSubject) {
    send_frame(socket, &ClientFrame::Subscribe { subject }).await;
}

async fn unsubscribe(socket: &mut Socket, subject: WsSubject) {
    send_frame(socket, &ClientFrame::Unsubscribe { subject }).await;
}

/// Reads one text frame within `READ_TIMEOUT` and parses it as a
/// `ServerEvent`. Panics instead of hanging if nothing arrives, so a broken
/// stream fails the test rather than the whole suite.
async fn recv_event(socket: &mut Socket) -> ServerEvent {
    let message = tokio::time::timeout(READ_TIMEOUT, socket.next())
        .await
        .expect("timed out waiting for a frame")
        .expect("socket closed before a frame arrived")
        .expect("read error");
    let text = message.into_text().expect("text frame");
    serde_json::from_str(&text).expect("frame is a valid ServerEvent")
}

/// Subscribes to `subject` and waits for its snapshot frame.
async fn subscribe_and_drain_snapshot(socket: &mut Socket, subject: WsSubject) -> ServerEvent {
    subscribe(socket, subject).await;
    recv_event(socket).await
}

/// Sends a frame guaranteed to draw an immediate reply, then waits for it.
async fn round_trip(socket: &mut Socket) {
    socket
        .send(Message::text("not json at all"))
        .await
        .expect("frame sends");
    let event = recv_event(socket).await;
    assert!(
        matches!(event, ServerEvent::Error(_)),
        "expected an error frame to round-trip: {event:?}"
    );
}

fn cluster_delta_event(upserted: Vec<ClusterRef>) -> ClusterDeltaEvent {
    ClusterDeltaEvent {
        upserted,
        removed: Vec::new(),
    }
}

/// Sorts by id so snapshot comparisons do not depend on the backing
/// `HashMap`'s iteration order.
fn sorted_ids(upserted: &[ClusterRef]) -> Vec<i64> {
    let mut ids: Vec<i64> = upserted.iter().map(|c| c.id).collect();
    ids.sort();
    ids
}

#[tokio::test(flavor = "multi_thread")]
async fn subscribing_to_cluster_delta_delivers_the_current_snapshot_first() {
    let deps = inert_deps();
    deps.cluster_delta_service().seed([
        ClusterRefFixture::new(1).with_txids(&["a", "b"]).build(),
        ClusterRefFixture::new(2).with_txids(&["c", "d"]).build(),
    ]);
    let port = spawn_server(&deps).await;
    let mut socket = connect(port).await;

    let event = subscribe_and_drain_snapshot(&mut socket, WsSubject::ClusterDelta).await;

    match event {
        ServerEvent::ClusterDelta(delta) => {
            assert_eq!(sorted_ids(&delta.upserted), vec![1, 2]);
            assert!(delta.removed.is_empty());
        }
        other => panic!("expected a cluster.delta snapshot, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn one_socket_multiplexes_several_subjects() {
    let deps = inert_deps();
    let port = spawn_server(&deps).await;
    let mut socket = connect(port).await;

    subscribe_and_drain_snapshot(&mut socket, WsSubject::ClusterDelta).await;
    subscribe_and_drain_snapshot(&mut socket, WsSubject::MempoolDelta).await;

    let cluster_event = cluster_delta_event(vec![ClusterRefFixture::new(1).build()]);
    let mempool_event = MempoolDeltaEventFixture::new().with_added(&["tx1"]).build();
    deps.pubsub
        .publish(Subject::ClusterDelta, &cluster_event)
        .await;
    deps.pubsub
        .publish(Subject::MempoolDelta, &mempool_event)
        .await;

    // Frames may interleave in either order -- assert on the set received,
    // not a fixed sequence.
    let mut got_cluster = false;
    let mut got_mempool = false;
    for _ in 0..2 {
        match recv_event(&mut socket).await {
            ServerEvent::ClusterDelta(delta) => {
                assert_eq!(sorted_ids(&delta.upserted), vec![1]);
                got_cluster = true;
            }
            ServerEvent::MempoolDelta(delta) => {
                assert_eq!(delta.added, vec!["tx1".to_string()]);
                got_mempool = true;
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }
    assert!(got_cluster, "cluster.delta frame never arrived");
    assert!(got_mempool, "mempool.delta frame never arrived");
}

#[tokio::test(flavor = "multi_thread")]
async fn unsubscribing_stops_only_that_subject() {
    let deps = inert_deps();
    let port = spawn_server(&deps).await;
    let mut socket = connect(port).await;

    subscribe_and_drain_snapshot(&mut socket, WsSubject::ClusterDelta).await;
    subscribe_and_drain_snapshot(&mut socket, WsSubject::MempoolDelta).await;

    unsubscribe(&mut socket, WsSubject::ClusterDelta).await;
    // Prove the unsubscribe was applied server-side before publishing, or the
    // still-running subscription task could catch the cluster.delta event.
    round_trip(&mut socket).await;

    let cluster_event = cluster_delta_event(vec![ClusterRefFixture::new(1).build()]);
    let mempool_event = MempoolDeltaEventFixture::new().with_added(&["tx1"]).build();
    deps.pubsub
        .publish(Subject::ClusterDelta, &cluster_event)
        .await;
    deps.pubsub
        .publish(Subject::MempoolDelta, &mempool_event)
        .await;

    match recv_event(&mut socket).await {
        ServerEvent::MempoolDelta(delta) => assert_eq!(delta.added, vec!["tx1".to_string()]),
        other => panic!("expected mempool.delta, got {other:?}"),
    }

    // No cluster.delta frame follows -- its subscription was torn down.
    let second = tokio::time::timeout(ABSENCE_TIMEOUT, socket.next()).await;
    assert!(
        second.is_err(),
        "expected no further frame, but got one after unsubscribing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_malformed_frame_is_reported_without_closing_the_socket() {
    let deps = inert_deps();
    let port = spawn_server(&deps).await;
    let mut socket = connect(port).await;

    round_trip(&mut socket).await;

    // The connection survived: it still answers a subscribe with a snapshot.
    let event = subscribe_and_drain_snapshot(&mut socket, WsSubject::ClusterDelta).await;
    assert!(matches!(event, ServerEvent::ClusterDelta(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn subscribing_twice_to_the_same_subject_is_a_no_op() {
    let deps = inert_deps();
    let port = spawn_server(&deps).await;
    let mut socket = connect(port).await;

    subscribe_and_drain_snapshot(&mut socket, WsSubject::ClusterDelta).await;
    subscribe(&mut socket, WsSubject::ClusterDelta).await;

    let second = tokio::time::timeout(ABSENCE_TIMEOUT, socket.next()).await;
    assert!(
        second.is_err(),
        "a second subscribe replayed a second snapshot frame"
    );
}
