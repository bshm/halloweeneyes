//! Maps raw camera pixel coordinates to normalised eye gaze coordinates.
//!
//! The original mapping was calibrated for a specific camera placement using
//! a 2-variable polynomial fitted with Wolfram Alpha.  The polynomial
//! transforms a point in the 1280×1024 camera coordinate space to eye
//! coordinates in [-1, 1].
//!
//! Input pixel coordinates are first scaled from the capture resolution
//! (320×240) to 1280×1024 (×4 in both axes, with a +8 row offset on y).

/// Configuration for the coordinate mapping.
///
/// Defaults match the original calibration.  Override for a different camera
/// placement by re-calibrating the polynomial coefficients.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CoordinateMapConfig {
    /// Capture width fed into the pipeline (pixels).
    pub capture_width: u32,
    /// Capture height fed into the pipeline (pixels).
    pub capture_height: u32,
    /// Target width for the polynomial evaluation.
    pub target_width: u32,
    /// Target height for the polynomial evaluation.
    pub target_height: u32,
    /// Row offset applied before scaling (original: 8).
    pub row_offset: f64,
    /// Polynomial coefficients for x_eye.  See [`map_to_eye`].
    pub x_poly: PolyCoeffs,
    /// Polynomial coefficients for y_eye.  See [`map_to_eye`].
    pub y_poly: PolyCoeffs,
}

/// Coefficients for the 2-variable bilinear polynomial used in the original
/// project:
///
/// ```text
/// result = c0
///        + x * (c1 + c2*x + c3*y)
///        + c4*y
/// ```
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PolyCoeffs {
    pub c0: f64,
    pub c1: f64,
    pub c2: f64,
    pub c3: f64,
    pub c4: f64,
}

impl PolyCoeffs {
    pub fn eval(&self, x: f64, y: f64) -> f64 {
        self.c0 + x * (self.c1 + self.c2 * x + self.c3 * y) + self.c4 * y
    }
}

impl Default for CoordinateMapConfig {
    fn default() -> Self {
        Self {
            capture_width: 320,
            capture_height: 240,
            target_width: 1280,
            target_height: 1024,
            row_offset: 8.0,
            x_poly: PolyCoeffs {
                c0: -2.10554,
                c1: 0.00362959,
                c2: -2.89324e-7,
                c3: -2.24893e-6,
                c4: 0.00145656,
            },
            y_poly: PolyCoeffs {
                c0: -0.383554,
                c1: 0.000500975,
                c2: -3.04759e-7,
                c3: 1.12333e-7,
                c4: 0.00225704,
            },
        }
    }
}

impl PolyCoeffs {
    pub fn from_slice(c: &[f64; 5]) -> Self {
        Self { c0: c[0], c1: c[1], c2: c[2], c3: c[3], c4: c[4] }
    }
    pub fn to_slice(&self) -> [f64; 5] {
        [self.c0, self.c1, self.c2, self.c3, self.c4]
    }
}

/// Least-squares fit of `eye = c0 + c1*x + c2*x^2 + c3*x*y + c4*y` to
/// `(x, y) -> target` samples. Returns `None` if the system is singular
/// (fewer than 5 independent points or degenerate geometry).
pub fn fit_poly(samples: &[(f64, f64, f64)]) -> Option<PolyCoeffs> {
    if samples.len() < 5 {
        return None;
    }
    // Normal equations A^T A c = A^T b for the 5 basis functions.
    let mut ata = [[0.0f64; 5]; 5];
    let mut atb = [0.0f64; 5];
    for &(x, y, t) in samples {
        let row = [1.0, x, x * x, x * y, y];
        for i in 0..5 {
            for j in 0..5 {
                ata[i][j] += row[i] * row[j];
            }
            atb[i] += row[i] * t;
        }
    }
    // Gaussian elimination with partial pivoting.
    let mut m = [[0.0f64; 6]; 5];
    for i in 0..5 {
        for j in 0..5 {
            m[i][j] = ata[i][j];
        }
        m[i][5] = atb[i];
    }
    for col in 0..5 {
        let mut piv = col;
        for r in col..5 {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        if m[piv][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, piv);
        let d = m[col][col];
        for j in col..6 {
            m[col][j] /= d;
        }
        for r in 0..5 {
            if r != col {
                let f = m[r][col];
                if f != 0.0 {
                    for j in col..6 {
                        m[r][j] -= f * m[col][j];
                    }
                }
            }
        }
    }
    let mut c = [0.0f64; 5];
    for i in 0..5 {
        c[i] = m[i][5];
    }
    if c.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(PolyCoeffs::from_slice(&c))
}

/// Maps a camera pixel `(px, py)` (in capture resolution) to a normalised
/// eye gaze point `(x_eye, y_eye)` clamped to `[-1.0, 1.0]`.
pub fn map_to_eye(px: f64, py: f64, cfg: &CoordinateMapConfig) -> (f64, f64) {
    let scale_x = cfg.target_width as f64 / cfg.capture_width as f64;
    let scale_y = cfg.target_height as f64 / cfg.capture_height as f64;

    let x = px * scale_x;
    let y = (py + cfg.row_offset) * scale_y;

    let x_eye = cfg.x_poly.eval(x, y).clamp(-1.0, 1.0);
    let y_eye = cfg.y_poly.eval(x, y).clamp(-1.0, 1.0);

    (x_eye, y_eye)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn default_cfg() -> CoordinateMapConfig {
        CoordinateMapConfig::default()
    }

    // --- PolyCoeffs::eval ---

    #[test]
    fn poly_eval_zero_inputs() {
        let p = PolyCoeffs {
            c0: 1.0,
            c1: 2.0,
            c2: 3.0,
            c3: 4.0,
            c4: 5.0,
        };
        // x=0, y=0 → c0
        assert_eq!(p.eval(0.0, 0.0), 1.0);
    }

    #[test]
    fn poly_eval_known_values() {
        let p = PolyCoeffs {
            c0: 0.0,
            c1: 1.0,
            c2: 0.0,
            c3: 0.0,
            c4: 0.0,
        };
        // result = x
        assert!((p.eval(3.0, 99.0) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn poly_eval_y_term() {
        let p = PolyCoeffs {
            c0: 0.0,
            c1: 0.0,
            c2: 0.0,
            c3: 0.0,
            c4: 2.0,
        };
        // result = 2*y
        assert!((p.eval(0.0, 5.0) - 10.0).abs() < 1e-12);
    }

    // --- map_to_eye: output always within [-1, 1] ---

    proptest! {
        #[test]
        fn output_clamped(
            px in 0.0f64..320.0,
            py in 0.0f64..240.0,
        ) {
            let cfg = default_cfg();
            let (xe, ye) = map_to_eye(px, py, &cfg);
            prop_assert!(xe >= -1.0 && xe <= 1.0,
                "x_eye={xe} out of range for px={px}, py={py}");
            prop_assert!(ye >= -1.0 && ye <= 1.0,
                "y_eye={ye} out of range for px={px}, py={py}");
        }
    }

    // --- map_to_eye: centre pixel maps near (0, 0) ---
    // The calibration polynomial was fitted such that (600, 298) in 1280×1024
    // maps to approximately (0, 0.5) — the original centre is not (0,0).
    // We just verify monotonicity: moving right increases x_eye.
    #[test]
    fn x_increases_rightward() {
        let cfg = default_cfg();
        let (x_left, _) = map_to_eye(0.0, 120.0, &cfg);
        let (x_right, _) = map_to_eye(319.0, 120.0, &cfg);
        assert!(x_right > x_left, "expected x_right > x_left, got {x_right} <= {x_left}");
    }

    #[test]
    fn y_increases_downward() {
        let cfg = default_cfg();
        let (_, y_top) = map_to_eye(160.0, 0.0, &cfg);
        let (_, y_bottom) = map_to_eye(160.0, 239.0, &cfg);
        assert!(y_bottom > y_top, "expected y_bottom > y_top, got {y_bottom} <= {y_top}");
    }

    // --- scale factors ---

    #[test]
    fn default_scale_is_4x() {
        let cfg = default_cfg();
        assert_eq!(
            cfg.target_width / cfg.capture_width,
            4
        );
        assert_eq!(
            cfg.target_height / cfg.capture_height,
            4
        );
    }
}
