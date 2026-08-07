mod streams;

use crate::infra::state::{AppRouter, AppState};
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use shared::ws::{ClientFrame, ServerEvent, WsError, WsSubject};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinHandle;

const PING_INTERVAL: Duration = Duration::from_secs(30);

/// How many frames may sit in the outbound queue before producers have to wait.
///
/// Every subscription task pushes here and the connection loop is the only
/// thing draining it into the socket. If the client reads slower than the
/// subjects produce, an unbounded queue would grow until the process runs out
/// of memory. Bounded instead: once it is full, `send` parks the subscription
/// task until the client catches up, so one slow client only degrades itself.
const OUTBOUND_CHANNEL_CAPACITY: usize = 256;

pub trait WebsocketControllerRouter {
    fn add_websocket_routes(self) -> Self;
}

impl WebsocketControllerRouter for AppRouter {
    fn add_websocket_routes(self) -> Self {
        self.route("/ws", get(ws_handler))
    }
}

/// Upgrades the connection to the multiplexed websocket, on which the client
/// subscribes/unsubscribes to named subjects rather than getting one socket
/// per subject.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_connection(socket, state))
}

async fn handle_connection(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Pre-serialized frames from every subscription task, fed to the socket
    // by this loop alone.
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(OUTBOUND_CHANNEL_CAPACITY);
    let mut subscriptions: HashMap<WsSubject, JoinHandle<()>> = HashMap::new();

    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.tick().await;

    loop {
        tokio::select! {
            outbound = outbound_rx.recv() => {
                let Some(text) = outbound else {
                    tracing::info!("Websocket closed: outbound channel closed");
                    break;
                };
                if let Err(e) = sender.send(Message::Text(text.into())).await {
                    tracing::info!("Websocket closed: send failed: {e}");
                    break;
                }
            }
            inbound = receiver.next() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => {
                        handle_client_frame(&text, &state, &outbound_tx, &mut subscriptions).await;
                    }
                    Some(Ok(Message::Close(frame))) => {
                        tracing::info!("Websocket closed: client sent Close ({frame:?})");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::info!("Websocket closed: receive error: {e}");
                        break;
                    }
                    None => {
                        tracing::info!("Websocket closed: client disconnected");
                        break;
                    }
                    Some(Ok(_)) => {}
                }
            }
            // Send a ping to the client every PING_INTERVAL to detect dead connections
            _ = ping.tick() => {
                if let Err(e) = sender.send(Message::Ping(Vec::new().into())).await {
                    tracing::info!("Websocket closed: ping failed: {e}");
                    break;
                }
            }
        }
    }

    // Nothing else will ever remove these once the connection loop is gone.
    for handle in subscriptions.into_values() {
        handle.abort();
    }
}

/// Parses one client frame and applies it: spawns/aborts the subscription
/// task for the named subject, or reports a parse failure back to the client
/// without closing the connection.
async fn handle_client_frame(
    text: &str,
    state: &AppState,
    outbound_tx: &mpsc::Sender<String>,
    subscriptions: &mut HashMap<WsSubject, JoinHandle<()>>,
) {
    let frame: ClientFrame = match serde_json::from_str(text) {
        Ok(frame) => frame,
        Err(e) => {
            send_event(
                outbound_tx,
                &ServerEvent::Error(WsError {
                    message: format!("invalid frame: {e}"),
                }),
            );
            return;
        }
    };

    match frame {
        ClientFrame::Subscribe { subject } => {
            if subscriptions.contains_key(&subject) {
                return;
            }
            tracing::info!("Websocket client subscribed to {subject}");
            let handle = spawn_subscription(subject, state.clone(), outbound_tx.clone());
            subscriptions.insert(subject, handle);
        }
        ClientFrame::Unsubscribe { subject } => {
            if let Some(handle) = subscriptions.remove(&subject) {
                handle.abort();
                tracing::info!("Websocket client unsubscribed from {subject}");
            }
        }
    }
}

/// Builds the subject's stream and feeds each event into the connection's
/// outbound channel.
fn spawn_subscription(
    subject: WsSubject,
    state: AppState,
    outbound_tx: mpsc::Sender<String>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut events = streams::build(subject, &state).await;
        while let Some(event) = events.next().await {
            let text = match serde_json::to_string(&event) {
                Ok(text) => text,
                Err(e) => {
                    tracing::error!("Failed to serialize event: {e}");
                    continue;
                }
            };
            if outbound_tx.send(text).await.is_err() {
                // The connection loop is gone; nothing left to serve.
                break;
            }
        }
    })
}

/// Queues an error frame for the connection loop to send.
fn send_event(outbound_tx: &mpsc::Sender<String>, event: &ServerEvent) {
    match serde_json::to_string(event) {
        Ok(text) => match outbound_tx.try_send(text) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::warn!("dropping error frame: outbound queue full");
            }
            Err(TrySendError::Closed(_)) => {
                tracing::error!("failed to queue error frame: receiver gone");
            }
        },
        Err(e) => tracing::error!("Failed to serialize event: {e}"),
    }
}
