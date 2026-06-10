use axum::extract::ws::{Message, WebSocket};
use futures::{Stream, StreamExt};
use serde::Serialize;

/// Serializes each item to JSON and sends it as a text frame until the
/// stream ends or the client disconnects.
pub async fn stream<S, T>(mut socket: WebSocket, stream: S)
where
    S: Stream<Item = T>,
    T: Serialize,
{
    futures::pin_mut!(stream);
    while let Some(item) = stream.next().await {
        let text = match serde_json::to_string(&item) {
            Ok(text) => text,
            Err(e) => {
                tracing::error!("Failed to serialize event: {e}");
                continue;
            }
        };
        if socket.send(Message::Text(text.into())).await.is_err() {
            break; // client disconnected
        }
    }
}
