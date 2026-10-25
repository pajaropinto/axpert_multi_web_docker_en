/// WebSocket handler — pushes SystemSnapshot to browser clients in real time.
use crate::web::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{debug, warn};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut snapshot_rx = state.snapshot_tx.subscribe();

    // Send the latest snapshot immediately if available
    {
        let snap = state.latest_snapshot.read().await;
        if let Some(ref s) = *snap {
            if let Ok(json) = serde_json::to_string(s) {
                let _ = sender.send(Message::Text(json)).await;
            }
        }
    }

    // Relay snapshots to client; also handle pings from client
    loop {
        tokio::select! {
            result = snapshot_rx.recv() => {
                match result {
                    Ok(snapshot) => {
                        match serde_json::to_string(&snapshot) {
                            Ok(json) => {
                                if sender.send(Message::Text(json)).await.is_err() {
                                    break; // Client disconnected
                                }
                            }
                            Err(e) => warn!("WS serialize error: {}", e),
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        debug!("WS client lagged {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }

            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    _ => {}
                }
            }
        }
    }
}
