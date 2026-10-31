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
const SIM_TICK_INTERVAL: Duration = Duration::from_millis(16);

/// Easing time constant for the broadcast state (detector-side, shared by all clients).
/// Stage 1: filter the raw motion target so per-frame centroid jumps are gone.
const TARGET_TAU: f64 = 0.12;

/// Stage 2: filter the already-smoothed target onto the output timeline.
const OUTPUT_TAU: f64 = 0.05;

/// Hard cap on how far the broadcast state may move per second, so a large
/// jump in the motion centroid (or motion flipping on/off) cannot snap the eye.
const MAX_SLEW_PER_SEC: f64 = 4.0;

/// If ffmpeg produces no frame for this long, the stream is considered dead and
/// the subprocess is restarted.
const FRAME_STALL_TIMEOUT: Duration = Duration::from_secs(10);

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
    let sim_start = Instant::now();
    let mut interval = tokio::time::interval(SIM_TICK_INTERVAL);
    let mut tick_count: u64 = 0;
    loop {
        interval.tick().await;
        let mut state = simulation_tick(&mut sim, Instant::now());
        state.t_ms = sim_start.elapsed().as_secs_f64() * 1000.0;
        tick_count += 1;
        if tick_count % 300 == 0 {
            debug!("sim tick {}: look=({:.2},{:.2}) blink={:.2}",
                tick_count, state.look_x, state.look_y, state.blink_level);
        }
        match tx.send(state) {
            Ok(n) => {
                if tick_count == 1 {
                    info!("first sim tick broadcast to {n} subscribers");
                }
            }
            Err(_) => debug!("sim broadcast: no subscribers"),
        }
    }
}

/// Camera + motion-detection loop backed by an ffmpeg subprocess.
async fn run_camera(
    cfg: CaptureConfig,
    tx: Arc<broadcast::Sender<EyeState>>,
) -> Result<()> {
    let rtsp_url = cfg.rtsp_url.as_deref().unwrap();
    info!("camera mode: rtsp_url={}", rtsp_url);
    let mut sim = EyeSimulation::new(crate::simulation::SimConfig::default());
    let frame_bytes = (cfg.width * cfg.height * 3) as usize;
    let mut prev_gray: Option<GrayImage> = None;
    let mut last_motion = Instant::now()
        .checked_sub(cfg.simulation_timeout)
        .unwrap_or(Instant::now());
    let start = Instant::now();
    let mut target = sim.tick(Instant::now());
    let mut filtered = target;
    let mut current = target;
    let mut last_tick = Instant::now();

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
        let mut last_frame = Instant::now();
        let mut ticker = tokio::time::interval(SIM_TICK_INTERVAL);

        loop {
            tokio::select! {
                read = stdout.read_exact(&mut buf) => {
                    if let Err(e) = read {
                        warn!("ffmpeg read error: {e}, restarting");
                        break;
                    }
                    last_frame = Instant::now();

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
                                    t_ms: 0.0,
                                    look_x: ex,
                                    look_y: ey,
                                    blink_level: sim.tick(now).blink_level,
                                    motion_detected: true,
                                }
                            }
                            None => sim.tick(now),
                        }
                    } else {
                        sim.tick(now)
                    };

                    prev_gray = Some(gray);
                    target = state;
                }
                _ = ticker.tick() => {
                    // Keep the eye alive even when no frames arrive: after the
                    // simulation timeout the autonomous animation takes over.
                    if last_frame.elapsed() > FRAME_STALL_TIMEOUT {
                        warn!("no frame for {:?} — restarting ffmpeg", last_frame.elapsed());
                        break;
                    }
                    if last_motion.elapsed() >= cfg.simulation_timeout {
                        target = sim.tick(Instant::now());
                    }

                    // Fixed-rate broadcast, independent of camera cadence: every
                    // client sees the same phase, and the easing happens here
                    // once instead of per client.
                    let now = Instant::now();
                    let dt = now.duration_since(last_tick).as_secs_f64();
                    last_tick = now;
                    // Stage 1: raw target -> filtered target (removes centroid jumps)
                    let a1 = 1.0 - (-dt / TARGET_TAU).exp();
                    filtered.look_x += (target.look_x - filtered.look_x) * a1;
                    filtered.look_y += (target.look_y - filtered.look_y) * a1;
                    filtered.blink_level += (target.blink_level - filtered.blink_level) * a1;
                    filtered.motion_detected = target.motion_detected;

                    // Stage 2: filtered target -> output (keeps the motion lively)
                    let a2 = 1.0 - (-dt / OUTPUT_TAU).exp();
                    let max_step = MAX_SLEW_PER_SEC * dt;
                    let step = |cur: f64, tgt: f64| {
                        let d = (tgt - cur) * a2;
                        d.clamp(-max_step, max_step)
                    };
                    current.look_x += step(current.look_x, filtered.look_x);
                    current.look_y += step(current.look_y, filtered.look_y);
                    current.blink_level += step(current.blink_level, filtered.blink_level);
                    current.motion_detected = target.motion_detected;
                    current.t_ms = start.elapsed().as_secs_f64() * 1000.0;
                    let _ = tx.send(current);
                }
            }
        }

        let _ = child.kill().await;
        let _ = child.wait().await;
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
