use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, Stream, StreamExt};
use serde::Serialize;
use std::time::Duration;

const PING_INTERVAL: Duration = Duration::from_secs(30);

/// Serializes each item to JSON and sends it as a text frame until the
/// stream ends or the client disconnects.
pub async fn stream<S, T>(socket: WebSocket, stream: S)
where
    S: Stream<Item = T>,
    T: Serialize,
{
    let (mut sender, mut receiver) = socket.split();
    futures::pin_mut!(stream);

    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.tick().await;

    loop {
        tokio::select! {
            item = stream.next() => {
                let Some(item) = item else {
                    tracing::info!("Websocket closed: stream ended");
                    break;
                };
                let text = match serde_json::to_string(&item) {
                    Ok(text) => text,
                    Err(e) => {
                        tracing::error!("Failed to serialize event: {e}");
                        continue;
                    }
                };
                if let Err(e) = sender.send(Message::Text(text.into())).await {
                    tracing::info!("Websocket closed: send failed: {e}");
                    break;
                }
            }
            inbound = receiver.next() => {
                match inbound {
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
}
