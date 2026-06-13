use serde::{Deserialize, Serialize};

/// Normalised eye state broadcast from detector to all renderers.
/// Both axes are in the range [-1.0, 1.0].
/// `blink_level` is in [0.0, 1.0]; 0 = fully open, 1 = fully closed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EyeState {
    /// Horizontal gaze direction, -1 = far left, +1 = far right.
    pub look_x: f64,
    /// Vertical gaze direction, -1 = up, +1 = down.
    pub look_y: f64,
    /// Eyelid closure level.
    pub blink_level: f64,
    /// True when state originates from live motion detection.
    pub motion_detected: bool,
}

impl EyeState {
    pub fn default_open() -> Self {
        Self {
            look_x: 0.0,
            look_y: 0.0,
            blink_level: 0.0,
            motion_detected: false,
        }
    }
}

/// Message sent over the WebSocket connection from the detector server
/// to browser clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    EyeUpdate(EyeState),
}
