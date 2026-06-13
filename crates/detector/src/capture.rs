//! RTSP video capture via an `ffmpeg` subprocess.
//!
//! `ffmpeg` is invoked with the RTSP URL and instructed to output raw
//! RGB24 frames at the configured resolution to stdout.  Each frame is
//! exactly `width * height * 3` bytes.  The subprocess is restarted
//! automatically if it exits unexpectedly.
//!
//! When a new frame arrives it is converted to grayscale and fed to the
//! motion detection pipeline.  Detected motion updates the shared
//! [`EyeSimulation`] and broadcasts an [`EyeState`] on the provided channel.
//!
//! When no motion is detected for [`CaptureConfig::simulation_timeout`] the
//! simulation resumes autonomous animation.
//!
//! Pass `rtsp_url = None` to skip the camera entirely and run the
//! simulation at ~30 fps with no camera dependency.

use crate::{coordinate::CoordinateMapConfig, motion::MotionConfig, simulation::EyeSimulation};
use anyhow::{Context, Result};
use image::GrayImage;
use shared::EyeState;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Target tick interval for simulation-only mode (~30 fps).
const SIM_TICK_INTERVAL: Duration = Duration::from_millis(33);

pub struct CaptureConfig {
    /// RTSP source URL understood by ffmpeg. `None` = simulation-only mode.
    pub rtsp_url: Option<String>,
    /// Capture width (must match motion pipeline expectation).
    pub width: u32,
    /// Capture height.
    pub height: u32,
    /// After this duration without detected motion, autonomous simulation resumes.
    pub simulation_timeout: Duration,
    pub motion: MotionConfig,
    pub coordinate: CoordinateMapConfig,
}

impl CaptureConfig {
    pub fn new(rtsp_url: Option<String>) -> Self {
        Self {
            rtsp_url,
            width: 320,
            height: 240,
            simulation_timeout: Duration::from_millis(2000),
            motion: MotionConfig::default(),
            coordinate: CoordinateMapConfig::default(),
        }
    }
}

/// Advance the simulation by one step and produce an [`EyeState`].
/// Extracted as a standalone function so it is testable without I/O.
pub fn simulation_tick(sim: &mut EyeSimulation<impl rand::Rng>, now: Instant) -> EyeState {
    sim.tick(now)
}

/// Runs the capture + detection loop, or a pure simulation loop when
/// `rtsp_url` is `None`.  Broadcasts [`EyeState`] on `tx`.
pub async fn run(
    rtsp_url: Option<String>,
    tx: Arc<broadcast::Sender<EyeState>>,
) -> Result<()> {
    let cfg = CaptureConfig::new(rtsp_url);
    match &cfg.rtsp_url {
        None => run_simulation_only(cfg, tx).await,
        Some(_) => run_camera(cfg, tx).await,
    }
}

/// Simulation-only loop: ticks at ~30 fps, no camera required.
async fn run_simulation_only(
    _cfg: CaptureConfig,
    tx: Arc<broadcast::Sender<EyeState>>,
) -> Result<()> {
    info!("no camera configured — running in simulation-only mode");
    let mut sim = EyeSimulation::new(crate::simulation::SimConfig::default());
    let mut interval = tokio::time::interval(SIM_TICK_INTERVAL);
    loop {
        interval.tick().await;
        let state = simulation_tick(&mut sim, Instant::now());
        let _ = tx.send(state);
    }
}

/// Camera + motion-detection loop backed by an ffmpeg subprocess.
async fn run_camera(
    cfg: CaptureConfig,
    tx: Arc<broadcast::Sender<EyeState>>,
) -> Result<()> {
    let rtsp_url = cfg.rtsp_url.as_deref().unwrap();
    let mut sim = EyeSimulation::new(crate::simulation::SimConfig::default());
    let frame_bytes = (cfg.width * cfg.height * 3) as usize;
    let mut prev_gray: Option<GrayImage> = None;
    let mut last_motion = Instant::now()
        .checked_sub(cfg.simulation_timeout)
        .unwrap_or(Instant::now());

    loop {
        info!("opening RTSP stream: {}", rtsp_url);

        let mut child = Command::new("ffmpeg")
            .args([
                "-loglevel", "error",
                "-rtsp_transport", "tcp",
                "-i", rtsp_url,
                "-vf", &format!("scale={}:{}", cfg.width, cfg.height),
                "-f", "rawvideo",
                "-pix_fmt", "rgb24",
                "-",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("failed to spawn ffmpeg — is it installed?")?;

        let mut stdout = child.stdout.take().context("no stdout on ffmpeg process")?;
        let mut buf = vec![0u8; frame_bytes];

        loop {
            match stdout.read_exact(&mut buf).await {
                Ok(_) => {}
                Err(e) => {
                    warn!("ffmpeg read error: {e}, restarting");
                    break;
                }
            }

            let gray = rgb24_to_gray(&buf, cfg.width, cfg.height);
            let now = Instant::now();

            let state = if let Some(ref prev) = prev_gray {
                match crate::motion::detect_motion(prev, &gray, &cfg.motion) {
                    Some(result) => {
                        last_motion = now;
                        let (ex, ey) = crate::coordinate::map_to_eye(
                            result.centroid_x,
                            result.centroid_y,
                            &cfg.coordinate,
                        );
                        sim.set_look(ex, ey);
                        debug!("motion at ({:.2},{:.2}) → eye ({ex:.3},{ey:.3})",
                            result.centroid_x, result.centroid_y);
                        EyeState {
                            look_x: ex,
                            look_y: ey,
                            blink_level: sim.tick(now).blink_level,
                            motion_detected: true,
                        }
                    }
                    None => {
                        let _ = last_motion; // suppress unused-variable warning when no motion
                        sim.tick(now)
                    }
                }
            } else {
                sim.tick(now)
            };

            prev_gray = Some(gray);
            let _ = tx.send(state);
        }

        let _ = child.kill().await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Converts a packed RGB24 buffer to a [`GrayImage`] using BT.601 luma.
fn rgb24_to_gray(buf: &[u8], width: u32, height: u32) -> GrayImage {
    let mut img = GrayImage::new(width, height);
    for (i, pixel) in buf.chunks_exact(3).enumerate() {
        let r = pixel[0] as u32;
        let g = pixel[1] as u32;
        let b = pixel[2] as u32;
        // BT.601: Y = (77*R + 150*G + 29*B) / 256
        let y = ((77 * r + 150 * g + 29 * b) >> 8) as u8;
        let x = (i as u32) % width;
        let row = (i as u32) / width;
        img.put_pixel(x, row, image::Luma([y]));
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // --- rgb24_to_gray ---

    #[test]
    fn gray_black_stays_black() {
        let buf = vec![0u8; 4 * 4 * 3];
        let img = rgb24_to_gray(&buf, 4, 4);
        for p in img.pixels() {
            assert_eq!(p[0], 0);
        }
    }

    #[test]
    fn gray_white_stays_near_255() {
        let buf = vec![255u8; 4 * 4 * 3];
        let img = rgb24_to_gray(&buf, 4, 4);
        for p in img.pixels() {
            // (77+150+29)/256 * 255 ≈ 254
            assert!(p[0] >= 254);
        }
    }

    proptest! {
        #[test]
        fn gray_output_range(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
            let buf = vec![r, g, b];
            let img = rgb24_to_gray(&buf, 1, 1);
            let v = img.get_pixel(0, 0)[0];
            // output must be a valid u8 (no panic)
            let _ = v;
        }
    }

    #[test]
    fn gray_dimensions_correct() {
        let buf = vec![0u8; 8 * 6 * 3];
        let img = rgb24_to_gray(&buf, 8, 6);
        assert_eq!(img.dimensions(), (8, 6));
    }

    // --- CaptureConfig ---

    #[test]
    fn capture_config_none_is_simulation_only() {
        let cfg = CaptureConfig::new(None);
        assert!(cfg.rtsp_url.is_none());
    }

    #[test]
    fn capture_config_some_stores_url() {
        let cfg = CaptureConfig::new(Some("rtsp://camera/live".to_string()));
        assert_eq!(cfg.rtsp_url.as_deref(), Some("rtsp://camera/live"));
    }

    // --- simulation_tick ---

    #[test]
    fn simulation_tick_output_in_range() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        let rng = StdRng::seed_from_u64(1);
        let mut sim = crate::simulation::EyeSimulation::with_rng(
            crate::simulation::SimConfig::default(),
            rng,
        );
        let state = simulation_tick(&mut sim, Instant::now());
        assert!(state.look_x >= -1.0 && state.look_x <= 1.0);
        assert!(state.look_y >= -1.0 && state.look_y <= 1.0);
        assert!(state.blink_level >= 0.0 && state.blink_level <= 1.0);
        assert!(!state.motion_detected);
    }
}
