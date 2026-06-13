//! Autonomous eye movement and blink simulation.
//!
//! Ported from the original `EyeSimulation.cpp` with the following changes:
//! - Pure-Rust, no Qt timers; driven by explicit `Instant` calls
//! - Injectable `Rng` for deterministic testing
//! - Separate left/right positions removed: both eyes track the same point
//!   (the original code did this too in practice)
//!
//! # State machine
//!
//! Eye movement and blinking run as independent finite state machines.
//! Call [`EyeSimulation::tick`] with the current `Instant` to advance both
//! machines and obtain an updated [`shared::EyeState`].

use rand::{Rng, RngExt, SeedableRng};
use shared::EyeState;
use std::time::{Duration, Instant};

/// Tunable parameters for the simulation.
#[derive(Debug, Clone)]
pub struct SimConfig {
    pub move_duration_min: Duration,
    pub move_duration_max: Duration,
    pub move_pause_min: Duration,
    pub move_pause_max: Duration,
    pub blink_duration_min: Duration,
    pub blink_duration_max: Duration,
    pub blink_pause_min: Duration,
    pub blink_pause_max: Duration,
    /// Maximum random noise added to blink_level per tick.
    pub blink_noise: f64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            move_duration_min: Duration::from_millis(72),
            move_duration_max: Duration::from_millis(144),
            move_pause_min: Duration::from_millis(150),
            move_pause_max: Duration::from_millis(3000),
            blink_duration_min: Duration::from_millis(40),
            blink_duration_max: Duration::from_millis(150),
            blink_pause_min: Duration::from_millis(300),
            blink_pause_max: Duration::from_millis(4000),
            blink_noise: 0.005,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum MovePhase {
    /// Waiting before next movement starts.
    Pausing { until: Instant },
    /// Actively interpolating to destination.
    Moving {
        started: Instant,
        duration: Duration,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum BlinkPhase {
    Pausing { until: Instant },
    Blinking {
        started: Instant,
        duration: Duration,
        from: f64,
        to: f64,
    },
}

/// Autonomous eye animation state machine.
pub struct EyeSimulation<R: Rng> {
    rng: R,
    cfg: SimConfig,
    move_phase: MovePhase,
    blink_phase: BlinkPhase,
    current_x: f64,
    current_y: f64,
    current_blink: f64,
}

impl EyeSimulation<rand::rngs::StdRng> {
    /// Creates a simulation seeded from the OS random source.
    pub fn new(cfg: SimConfig) -> Self {
        Self::with_rng(cfg, rand::rngs::StdRng::from_seed(rand::random::<[u8; 32]>()))
    }
}

impl<R: Rng> EyeSimulation<R> {
    pub fn with_rng(cfg: SimConfig, rng: R) -> Self {
        let now = Instant::now();
        let move_pause = Duration::from_millis(150);
        let blink_pause = Duration::from_millis(300);
        Self {
            rng,
            cfg,
            move_phase: MovePhase::Pausing {
                until: now + move_pause,
            },
            blink_phase: BlinkPhase::Pausing {
                until: now + blink_pause,
            },
            current_x: 0.0,
            current_y: 0.0,
            current_blink: 0.0,
        }
    }

    /// Advance the simulation to `now` and return the current [`EyeState`].
    pub fn tick(&mut self, now: Instant) -> EyeState {
        self.tick_movement(now);
        self.tick_blink(now);
        EyeState {
            look_x: self.current_x,
            look_y: self.current_y,
            blink_level: self.current_blink,
            motion_detected: false,
        }
    }

    /// Override gaze position from external motion detection.
    pub fn set_look(&mut self, x: f64, y: f64) {
        self.current_x = x.clamp(-1.0, 1.0);
        self.current_y = y.clamp(-1.0, 1.0);
    }

    fn rand_duration(&mut self, min: Duration, max: Duration) -> Duration {
        let min_ms = min.as_millis() as u64;
        let max_ms = max.as_millis() as u64;
        Duration::from_millis(self.rng.random_range(min_ms..=max_ms))
    }

    fn rand_f64(&mut self, min: f64, max: f64) -> f64 {
        self.rng.random_range(min..=max)
    }

    fn tick_movement(&mut self, now: Instant) {
        loop {
            match &self.move_phase.clone() {
                MovePhase::Pausing { until } => {
                    if now < *until {
                        break;
                    }
                    let duration =
                        self.rand_duration(self.cfg.move_duration_min, self.cfg.move_duration_max);
                    let to_x = self.rand_f64(-1.0, 1.0);
                    // Original: don't look upward, so y in [0, 1]
                    let to_y = self.rand_f64(0.0, 1.0);
                    self.move_phase = MovePhase::Moving {
                        started: now,
                        duration,
                        from_x: self.current_x,
                        from_y: self.current_y,
                        to_x,
                        to_y,
                    };
                }
                MovePhase::Moving {
                    started,
                    duration,
                    from_x,
                    from_y,
                    to_x,
                    to_y,
                } => {
                    let elapsed = now.saturating_duration_since(*started);
                    if elapsed >= *duration {
                        self.current_x = *to_x;
                        self.current_y = *to_y;
                        let pause =
                            self.rand_duration(self.cfg.move_pause_min, self.cfg.move_pause_max);
                        self.move_phase = MovePhase::Pausing { until: now + pause };
                        break;
                    }
                    let t = elapsed.as_secs_f64() / duration.as_secs_f64();
                    let smoothed = smooth_step(t);
                    self.current_x = lerp(smoothed, *from_x, *to_x);
                    self.current_y = lerp(smoothed, *from_y, *to_y);
                    break;
                }
            }
        }
    }

    fn tick_blink(&mut self, now: Instant) {
        loop {
            match &self.blink_phase.clone() {
                BlinkPhase::Pausing { until } => {
                    if now < *until {
                        break;
                    }
                    let duration = self
                        .rand_duration(self.cfg.blink_duration_min, self.cfg.blink_duration_max);
                    let to = self.rand_f64(0.0, 0.5);
                    self.blink_phase = BlinkPhase::Blinking {
                        started: now,
                        duration,
                        from: self.current_blink,
                        to,
                    };
                }
                BlinkPhase::Blinking {
                    started,
                    duration,
                    from,
                    to,
                } => {
                    let elapsed = now.saturating_duration_since(*started);
                    if elapsed >= *duration {
                        self.current_blink = *to;
                        let pause =
                            self.rand_duration(self.cfg.blink_pause_min, self.cfg.blink_pause_max);
                        self.blink_phase = BlinkPhase::Pausing { until: now + pause };
                        break;
                    }
                    let t = elapsed.as_secs_f64() / duration.as_secs_f64();
                    // Original: fast close (0..1/3 of duration), slow open (1/3..1)
                    let blink_level = if t <= 1.0 / 3.0 {
                        let f = t * 3.0;
                        lerp(f, *from, 1.0)
                    } else {
                        let f = 1.5 * t - 0.5;
                        lerp(f, 1.0, *to)
                    };
                    self.current_blink =
                        (blink_level + self.rand_f64(-self.cfg.blink_noise, self.cfg.blink_noise))
                            .clamp(0.0, 1.0);
                    break;
                }
            }
        }
    }
}

/// Cubic smoothstep: `3t² - 2t³`
fn smooth_step(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rand::rngs::StdRng;

    fn deterministic_sim() -> EyeSimulation<StdRng> {
        let rng = StdRng::seed_from_u64(42);
        EyeSimulation::with_rng(SimConfig::default(), rng)
    }

    // --- smooth_step ---

    #[test]
    fn smooth_step_endpoints() {
        assert!((smooth_step(0.0) - 0.0).abs() < 1e-12);
        assert!((smooth_step(1.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn smooth_step_midpoint() {
        assert!((smooth_step(0.5) - 0.5).abs() < 1e-12);
    }

    proptest! {
        #[test]
        fn smooth_step_monotone(a in 0.0f64..1.0, b in 0.0f64..1.0) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            prop_assert!(smooth_step(lo) <= smooth_step(hi) + 1e-12);
        }

        #[test]
        fn smooth_step_range(t in 0.0f64..=1.0) {
            let v = smooth_step(t);
            prop_assert!(v >= 0.0 && v <= 1.0);
        }
    }

    // --- lerp ---

    #[test]
    fn lerp_at_zero() {
        assert!((lerp(0.0, 3.0, 7.0) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn lerp_at_one() {
        assert!((lerp(1.0, 3.0, 7.0) - 7.0).abs() < 1e-12);
    }

    #[test]
    fn lerp_midpoint() {
        assert!((lerp(0.5, 0.0, 10.0) - 5.0).abs() < 1e-12);
    }

    // --- EyeSimulation::tick: output ranges ---

    proptest! {
        #[test]
        fn tick_look_clamped(advance_ms in 0u64..10_000) {
            let rng = StdRng::seed_from_u64(7);
            let mut sim = EyeSimulation::with_rng(SimConfig::default(), rng);
            let now = Instant::now() + Duration::from_millis(advance_ms);
            let state = sim.tick(now);
            prop_assert!(state.look_x >= -1.0 && state.look_x <= 1.0);
            prop_assert!(state.look_y >= -1.0 && state.look_y <= 1.0);
        }

        #[test]
        fn tick_blink_clamped(advance_ms in 0u64..10_000) {
            let rng = StdRng::seed_from_u64(99);
            let mut sim = EyeSimulation::with_rng(SimConfig::default(), rng);
            let now = Instant::now() + Duration::from_millis(advance_ms);
            let state = sim.tick(now);
            prop_assert!(state.blink_level >= 0.0 && state.blink_level <= 1.0);
        }
    }

    // --- tick: motion_detected is always false in simulation mode ---

    #[test]
    fn tick_motion_detected_false() {
        let mut sim = deterministic_sim();
        let state = sim.tick(Instant::now() + Duration::from_millis(5000));
        assert!(!state.motion_detected);
    }

    // --- set_look: clamps to [-1, 1] ---

    proptest! {
        #[test]
        fn set_look_clamps(x in -10.0f64..10.0, y in -10.0f64..10.0) {
            let mut sim = deterministic_sim();
            sim.set_look(x, y);
            let now = Instant::now();
            // Pause phases are in the future, so tick won't override our set
            let state = sim.tick(now);
            prop_assert!(state.look_x >= -1.0 && state.look_x <= 1.0);
            prop_assert!(state.look_y >= -1.0 && state.look_y <= 1.0);
        }
    }

    // --- eye does not look upward in simulation (y >= 0) ---

    #[test]
    fn simulation_does_not_look_up() {
        let rng = StdRng::seed_from_u64(12345);
        let mut sim = EyeSimulation::with_rng(SimConfig::default(), rng);
        let base = Instant::now();
        // Sample 200 ticks spread over 60 seconds
        for i in 0..200u64 {
            let now = base + Duration::from_millis(i * 300);
            let state = sim.tick(now);
            assert!(
                state.look_y >= -0.005, // small noise tolerance
                "look_y={} should be >= 0 in simulation mode",
                state.look_y
            );
        }
    }
}
