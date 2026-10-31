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
    Json,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use shared::{EyeState, ServerMessage};
use crate::coordinate::{fit_poly, CoordinateMapConfig, PolyCoeffs};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{debug, info};

type Samples = Arc<Mutex<Vec<(f64, f64, f64, f64)>>>;

#[derive(Clone)]
pub struct CalibState {
    pub map: Arc<RwLock<CoordinateMapConfig>>,
    pub samples: Samples,
    pub frame: Arc<Mutex<Option<Vec<u8>>>>,
}

#[derive(Clone)]
struct AppState {
    tx: Arc<broadcast::Sender<EyeState>>,
    start: Instant,
    calib: CalibState,
}

pub async fn run(
    bind_addr: String,
    tx: Arc<broadcast::Sender<EyeState>>,
    calib: CalibState,
) -> Result<()> {
    let state = AppState { tx, start: Instant::now(), calib };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/time", get(time_handler))
        .route("/frame.jpg", get(frame_handler))
        .route("/calib", get(calib_page))
        .route("/calib/state", get(calib_state))
        .route("/calib", post(calib_set))
        .route("/calib/sample", post(calib_sample))
        .route("/calib/fit", post(calib_fit))
        .route("/calib/reset", post(calib_reset))
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

/// Latest camera frame as JPEG, for the calibration page.
async fn frame_handler(State(state): State<AppState>) -> impl IntoResponse {
    let body = state.calib.frame.lock().unwrap().clone();
    match body {
        Some(b) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "image/jpeg")],
            b,
        ),
        None => (StatusCode::SERVICE_UNAVAILABLE, [(axum::http::header::CONTENT_TYPE, "text/plain")], Vec::new()),
    }
}

async fn calib_page() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../static/calib.html"),
    )
}

#[derive(Serialize)]
struct CalibOut {
    x: Vec<f64>,
    y: Vec<f64>,
    row_offset: f64,
    samples: usize,
}

fn calib_out(cfg: &CoordinateMapConfig, samples: usize) -> CalibOut {
    CalibOut {
        x: cfg.x_poly.to_slice().to_vec(),
        y: cfg.y_poly.to_slice().to_vec(),
        row_offset: cfg.row_offset,
        samples,
    }
}

async fn calib_state(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = state.calib.map.read().unwrap().clone();
    let n = state.calib.samples.lock().unwrap().len();
    Json(calib_out(&cfg, n))
}

#[derive(Deserialize)]
struct CalibIn {
    x: Vec<f64>,
    y: Vec<f64>,
    #[serde(default)]
    row_offset: Option<f64>,
}

async fn calib_set(State(state): State<AppState>, Json(body): Json<CalibIn>) -> impl IntoResponse {
    if body.x.len() != 5 || body.y.len() != 5 || body.x.iter().chain(body.y.iter()).any(|v| !v.is_finite()) {
        return (StatusCode::BAD_REQUEST, "expected 5 finite coefficients per axis".to_string());
    }
    let mut cfg = state.calib.map.write().unwrap();
    if let Some(off) = body.row_offset {
        if off.is_finite() {
            cfg.row_offset = off;
        }
    }
    cfg.x_poly = PolyCoeffs::from_slice(&[body.x[0], body.x[1], body.x[2], body.x[3], body.x[4]]);
    cfg.y_poly = PolyCoeffs::from_slice(&[body.y[0], body.y[1], body.y[2], body.y[3], body.y[4]]);
    drop(cfg);
    let cfg = state.calib.map.read().unwrap().clone();
    let n = state.calib.samples.lock().unwrap().len();
    (StatusCode::OK, serde_json::to_string(&calib_out(&cfg, n)).unwrap())
}

#[derive(Deserialize)]
struct SampleIn {
    camera: [f64; 2],
    eye: [f64; 2],
}

async fn calib_sample(State(state): State<AppState>, Json(body): Json<SampleIn>) -> impl IntoResponse {
    let cfg = state.calib.map.read().unwrap().clone();
    let sx = cfg.target_width as f64 / cfg.capture_width as f64;
    let sy = cfg.target_height as f64 / cfg.capture_height as f64;
    let x = body.camera[0] * sx;
    let y = (body.camera[1] + cfg.row_offset) * sy;
    if !x.is_finite() || !y.is_finite() {
        return (StatusCode::BAD_REQUEST, "non-finite sample".to_string());
    }
    let mut s = state.calib.samples.lock().unwrap();
    s.push((x, y, body.eye[0], body.eye[1]));
    (StatusCode::OK, format!("sample added, total {}", s.len()))
}

async fn calib_fit(State(state): State<AppState>) -> impl IntoResponse {
    let samples = state.calib.samples.lock().unwrap().clone();
    let xs: Vec<(f64, f64, f64)> = samples.iter().map(|s| (s.0, s.1, s.2)).collect();
    let ys: Vec<(f64, f64, f64)> = samples.iter().map(|s| (s.0, s.1, s.3)).collect();
    let (Some(px), Some(py)) = (fit_poly(&xs), fit_poly(&ys)) else {
        return (StatusCode::BAD_REQUEST, format!("need >=5 independent samples, have {}", samples.len()));
    };
    {
        let mut cfg = state.calib.map.write().unwrap();
        cfg.x_poly = px;
        cfg.y_poly = py;
    }
    let cfg = state.calib.map.read().unwrap().clone();
    let mut err = 0.0;
    for s in &samples {
        let (ex, ey) = crate::coordinate::map_to_eye(
            s.0 / (cfg.target_width as f64 / cfg.capture_width as f64),
            s.1 / (cfg.target_height as f64 / cfg.capture_height as f64) - cfg.row_offset,
            &cfg,
        );
        err += (ex - s.2).powi(2) + (ey - s.3).powi(2);
    }
    let rms = (err / (2.0 * samples.len().max(1) as f64)).sqrt();
    (
        StatusCode::OK,
        format!(
            "fit applied: {} samples, rms {:.4}\n{}",
            samples.len(),
            rms,
            serde_json::to_string(&calib_out(&cfg, samples.len())).unwrap()
        ),
    )
}

async fn calib_reset(State(state): State<AppState>) -> impl IntoResponse {
    {
        let mut cfg = state.calib.map.write().unwrap();
        *cfg = CoordinateMapConfig::default();
    }
    state.calib.samples.lock().unwrap().clear();
    (StatusCode::OK, "reset to defaults".to_string())
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
    use serde::{Deserialize, Serialize};
use shared::{EyeState, ServerMessage};
use crate::coordinate::{fit_poly, CoordinateMapConfig, PolyCoeffs};

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
