//! axum-based HTTP + WebSocket server.
//!
//! Routes:
//!   `GET /`          → serves the frontend `index.html`
//!   `GET /ws`        → WebSocket upgrade; streams [`ServerMessage::EyeUpdate`]
//!                      JSON frames to all connected clients
//!   `GET /assets/*`  → static assets built by trunk (WASM, JS glue, etc.)

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use shared::{EyeState, ServerMessage};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{debug, info};

#[derive(Clone)]
struct AppState {
    tx: Arc<broadcast::Sender<EyeState>>,
    start: Instant,
}

pub async fn run(bind_addr: String, tx: Arc<broadcast::Sender<EyeState>>) -> Result<()> {
    let state = AppState { tx, start: Instant::now() };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/time", get(time_handler))
        .route("/log", post(frontend_log_handler))
        .fallback_service(ServeDir::new("crates/frontend/dist"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("bind {bind_addr}: {e}"))?;

    info!("listening on http://{bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Detector monotonic clock in milliseconds; clients use this to map their
/// local timeline onto the server's so both eyes render the same phase.
async fn time_handler(State(state): State<AppState>) -> impl IntoResponse {
    let t_ms = state.start.elapsed().as_secs_f64() * 1000.0;
    (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "application/json")],
     format!("{{\"t_ms\":{t_ms}}}"))
}

/// Receives plain-text log lines from the WASM frontend and prints them
/// via tracing so they appear interleaved with backend logs.
async fn frontend_log_handler(body: Bytes) -> StatusCode {
    let msg = String::from_utf8_lossy(&body);
    info!(target: "frontend", "{}", msg.trim());
    StatusCode::NO_CONTENT
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    info!("new WebSocket upgrade request");
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    info!("WebSocket client connected");
    let mut rx = state.tx.subscribe();
    let mut sent = 0u64;
    loop {
        match rx.recv().await {
            Ok(eye_state) => {
                let msg = ServerMessage::EyeUpdate(eye_state);
                match serde_json::to_string(&msg) {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            info!("WebSocket client disconnected after {sent} messages");
                            break;
                        }
                        sent += 1;
                        if sent % 300 == 0 {
                            debug!("sent {sent} messages to ws client; last eye={:?}", eye_state);
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
            t_ms: 0.0,
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
