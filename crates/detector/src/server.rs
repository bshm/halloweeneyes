//! axum-based HTTP + WebSocket server.
//!
//! Routes:
//!   `GET /`          → serves the frontend `index.html`
//!   `GET /ws`        → WebSocket upgrade; streams [`ServerMessage::EyeUpdate`]
//!                      JSON frames to all connected clients
//!   `GET /assets/*`  → static assets built by trunk (WASM, JS glue, etc.)

use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use shared::{EyeState, ServerMessage};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{debug, info};

#[derive(Clone)]
struct AppState {
    tx: Arc<broadcast::Sender<EyeState>>,
}

pub async fn run(bind_addr: String, tx: Arc<broadcast::Sender<EyeState>>) -> Result<()> {
    let state = AppState { tx };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("frontend/dist"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("bind {bind_addr}: {e}"))?;

    info!("listening on http://{bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();
    loop {
        match rx.recv().await {
            Ok(eye_state) => {
                let msg = ServerMessage::EyeUpdate(eye_state);
                match serde_json::to_string(&msg) {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            debug!("websocket client disconnected");
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("serialise error: {e}");
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                debug!("ws subscriber lagged by {n} messages");
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use shared::{EyeState, ServerMessage};

    #[test]
    fn server_message_serialises() {
        let state = EyeState {
            look_x: 0.5,
            look_y: -0.3,
            blink_level: 0.1,
            motion_detected: true,
        };
        let msg = ServerMessage::EyeUpdate(state);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("eye_update"));
        assert!(json.contains("look_x"));
    }

    #[test]
    fn server_message_deserialises() {
        let json = r#"{"type":"eye_update","look_x":0.5,"look_y":-0.3,"blink_level":0.1,"motion_detected":true}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        let ServerMessage::EyeUpdate(s) = msg;
        assert!((s.look_x - 0.5).abs() < 1e-9);
    }
}
