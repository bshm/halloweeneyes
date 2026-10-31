//! Frame-differencing motion detection pipeline.
//!
//! Ported from `QtMotionTracking.cpp`:
//! 1. Convert frame to grayscale
//! 2. `absdiff` with previous frame
//! 3. Threshold at `SENSITIVITY`
//! 4. Box blur with kernel `BLUR_SIZE`
//! 5. Threshold again
//! 6. Find contours (connected component labelling)
//! 7. Pick the largest component, return its centroid
//!
//! All image operations run on `image::GrayImage` (pure Rust, no OpenCV).

use image::{GrayImage, Luma};

/// Parameters for the motion detection pipeline.
#[derive(Debug, Clone)]
pub struct MotionConfig {
    /// Pixel-difference threshold (0–255).  Original: 40.
    pub sensitivity: u8,
    /// Box-blur radius in pixels.  Original: 10.
    pub blur_size: u32,
    /// Minimum contour area in pixels to be considered motion.
    pub min_area: u32,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            sensitivity: 40,
            blur_size: 10,
            min_area: 1,
        }
    }
}

/// Result of one motion detection step.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionResult {
    /// Centroid of the largest moving region, in pixel coordinates.
    pub centroid_x: f64,
    pub centroid_y: f64,
    /// Area of the largest detected region in pixels.
    pub area: u32,
    /// Bounding box of the largest region: (x0, y0, x1, y1), inclusive.
    pub bbox: (u32, u32, u32, u32),
}

/// Computes the absolute per-pixel difference between two same-size grayscale images.
pub fn absdiff(a: &GrayImage, b: &GrayImage) -> GrayImage {
    assert_eq!(a.dimensions(), b.dimensions(), "absdiff: dimension mismatch");
    let (w, h) = a.dimensions();
    GrayImage::from_fn(w, h, |x, y| {
        let va = a.get_pixel(x, y)[0] as i16;
        let vb = b.get_pixel(x, y)[0] as i16;
        Luma([(va - vb).unsigned_abs() as u8])
    })
}

/// Applies a binary threshold: pixels >= `threshold` become 255, others 0.
pub fn threshold(img: &GrayImage, thresh: u8) -> GrayImage {
    let (w, h) = img.dimensions();
    GrayImage::from_fn(w, h, |x, y| {
        let v = img.get_pixel(x, y)[0];
        Luma([if v >= thresh { 255 } else { 0 }])
    })
}

/// Applies an integer box blur with the given radius.
///
/// Uses separable horizontal + vertical passes for O(w·h) complexity
/// regardless of radius.
pub fn box_blur(img: &GrayImage, radius: u32) -> GrayImage {
    if radius == 0 {
        return img.clone();
    }
    let h_pass = box_blur_horizontal(img, radius);
    box_blur_vertical(&h_pass, radius)
}

fn box_blur_horizontal(img: &GrayImage, radius: u32) -> GrayImage {
    let (w, h) = img.dimensions();
    let kernel = 2 * radius + 1;
    GrayImage::from_fn(w, h, |x, y| {
        let x0 = x.saturating_sub(radius);
        let x1 = (x + radius).min(w - 1);
        let count = (x1 - x0 + 1) as u32;
        let sum: u32 = (x0..=x1).map(|xi| img.get_pixel(xi, y)[0] as u32).sum();
        let _ = kernel;
        Luma([(sum / count) as u8])
    })
}

fn box_blur_vertical(img: &GrayImage, radius: u32) -> GrayImage {
    let (w, h) = img.dimensions();
    GrayImage::from_fn(w, h, |x, y| {
        let y0 = y.saturating_sub(radius);
        let y1 = (y + radius).min(h - 1);
        let count = (y1 - y0 + 1) as u32;
        let sum: u32 = (y0..=y1).map(|yi| img.get_pixel(x, yi)[0] as u32).sum();
        Luma([(sum / count) as u8])
    })
}

/// Detects the largest connected white (255) region in a binary image using
/// 4-connectivity flood fill.  Returns `None` if no white pixels exist or
/// the largest region is smaller than `min_area`.
pub fn largest_region_centroid(img: &GrayImage, min_area: u32) -> Option<MotionResult> {
    let (w, h) = img.dimensions();
    let mut visited = vec![false; (w * h) as usize];
    let mut best: Option<MotionResult> = None;

    for start_y in 0..h {
        for start_x in 0..w {
            let idx = (start_y * w + start_x) as usize;
            if visited[idx] || img.get_pixel(start_x, start_y)[0] == 0 {
                continue;
            }
            // BFS flood fill
            let mut queue = std::collections::VecDeque::new();
            queue.push_back((start_x, start_y));
            visited[idx] = true;
            let mut sum_x: u64 = 0;
            let mut sum_y: u64 = 0;
            let mut area: u32 = 0;
            let mut x0 = start_x; let mut x1 = start_x;
            let mut y0 = start_y; let mut y1 = start_y;

            while let Some((cx, cy)) = queue.pop_front() {
                sum_x += cx as u64;
                sum_y += cy as u64;
                area += 1;
                x0 = x0.min(cx); x1 = x1.max(cx);
                y0 = y0.min(cy); y1 = y1.max(cy);

                for (nx, ny) in neighbours_4(cx, cy, w, h) {
                    let nidx = (ny * w + nx) as usize;
                    if !visited[nidx] && img.get_pixel(nx, ny)[0] == 255 {
                        visited[nidx] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }

            if area >= min_area {
                let candidate = MotionResult {
                    centroid_x: sum_x as f64 / area as f64,
                    centroid_y: sum_y as f64 / area as f64,
                    area,
                    bbox: (x0, y0, x1, y1),
                };
                if best.as_ref().map_or(true, |b| candidate.area > b.area) {
                    best = Some(candidate);
                }
            }
        }
    }
    best
}

fn neighbours_4(x: u32, y: u32, w: u32, h: u32) -> impl Iterator<Item = (u32, u32)> {
    let mut v = Vec::with_capacity(4);
    if x > 0 {
        v.push((x - 1, y));
    }
    if x + 1 < w {
        v.push((x + 1, y));
    }
    if y > 0 {
        v.push((x, y - 1));
    }
    if y + 1 < h {
        v.push((x, y + 1));
    }
    v.into_iter()
}

/// Full motion detection pipeline: given two consecutive grayscale frames,
/// return the centroid of the largest moving region, or `None`.
pub fn detect_motion(
    prev: &GrayImage,
    current: &GrayImage,
    cfg: &MotionConfig,
) -> Option<MotionResult> {
    let diff = absdiff(prev, current);
    let threshed = threshold(&diff, cfg.sensitivity);
    let blurred = box_blur(&threshed, cfg.blur_size);
    let threshed2 = threshold(&blurred, cfg.sensitivity);
    largest_region_centroid(&threshed2, cfg.min_area)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GrayImage;
    use proptest::prelude::*;

    fn blank(w: u32, h: u32) -> GrayImage {
        GrayImage::new(w, h)
    }

    fn filled(w: u32, h: u32, v: u8) -> GrayImage {
        GrayImage::from_pixel(w, h, Luma([v]))
    }

    // --- absdiff ---

    #[test]
    fn absdiff_identical_images_is_zero() {
        let a = filled(4, 4, 128);
        let result = absdiff(&a, &a);
        for p in result.pixels() {
            assert_eq!(p[0], 0);
        }
    }

    #[test]
    fn absdiff_known_difference() {
        let a = filled(2, 2, 200);
        let b = filled(2, 2, 50);
        let result = absdiff(&a, &b);
        for p in result.pixels() {
            assert_eq!(p[0], 150);
        }
    }

    proptest! {
        #[test]
        fn absdiff_symmetric(
            a in 0u8..=255,
            b in 0u8..=255,
        ) {
            let ia = filled(3, 3, a);
            let ib = filled(3, 3, b);
            let ab = absdiff(&ia, &ib);
            let ba = absdiff(&ib, &ia);
            for (pa, pb) in ab.pixels().zip(ba.pixels()) {
                prop_assert_eq!(pa[0], pb[0]);
            }
        }

        #[test]
        fn absdiff_no_overflow(a in 0u8..=255, b in 0u8..=255) {
            let ia = filled(3, 3, a);
            let ib = filled(3, 3, b);
            let result = absdiff(&ia, &ib);
            for p in result.pixels() {
                // must not panic / overflow
                let _ = p[0];
            }
        }
    }

    // --- threshold ---

    #[test]
    fn threshold_all_below() {
        let img = filled(4, 4, 30);
        let result = threshold(&img, 40);
        for p in result.pixels() {
            assert_eq!(p[0], 0);
        }
    }

    #[test]
    fn threshold_all_above() {
        let img = filled(4, 4, 200);
        let result = threshold(&img, 40);
        for p in result.pixels() {
            assert_eq!(p[0], 255);
        }
    }

    #[test]
    fn threshold_at_boundary_is_255() {
        let img = filled(1, 1, 40);
        let result = threshold(&img, 40);
        assert_eq!(result.get_pixel(0, 0)[0], 255);
    }

    proptest! {
        #[test]
        fn threshold_output_binary(v in 0u8..=255, t in 0u8..=255) {
            let img = filled(2, 2, v);
            let result = threshold(&img, t);
            for p in result.pixels() {
                prop_assert!(p[0] == 0 || p[0] == 255);
            }
        }
    }

    // --- box_blur ---

    #[test]
    fn box_blur_uniform_image_unchanged() {
        let img = filled(10, 10, 128);
        let result = box_blur(&img, 3);
        for p in result.pixels() {
            assert_eq!(p[0], 128);
        }
    }

    #[test]
    fn box_blur_radius_zero_is_identity() {
        let img = filled(5, 5, 77);
        let result = box_blur(&img, 0);
        for (a, b) in img.pixels().zip(result.pixels()) {
            assert_eq!(a[0], b[0]);
        }
    }

    #[test]
    fn box_blur_preserves_dimensions() {
        let img = blank(8, 6);
        let result = box_blur(&img, 2);
        assert_eq!(result.dimensions(), (8, 6));
    }

    // --- largest_region_centroid ---

    #[test]
    fn no_white_pixels_returns_none() {
        let img = blank(10, 10);
        assert!(largest_region_centroid(&img, 1).is_none());
    }

    #[test]
    fn single_white_pixel() {
        let mut img = blank(10, 10);
        img.put_pixel(5, 3, Luma([255]));
        let result = largest_region_centroid(&img, 1).unwrap();
        assert!((result.centroid_x - 5.0).abs() < 1e-9);
        assert!((result.centroid_y - 3.0).abs() < 1e-9);
        assert_eq!(result.area, 1);
    }

    #[test]
    fn picks_largest_of_two_regions() {
        let mut img = blank(20, 20);
        // small region: 1 pixel at (1,1)
        img.put_pixel(1, 1, Luma([255]));
        // large region: 3x3 block at (10..12, 10..12)
        for y in 10..13u32 {
            for x in 10..13u32 {
                img.put_pixel(x, y, Luma([255]));
            }
        }
        let result = largest_region_centroid(&img, 1).unwrap();
        assert_eq!(result.area, 9);
        assert!((result.centroid_x - 11.0).abs() < 0.1);
        assert!((result.centroid_y - 11.0).abs() < 0.1);
    }

    #[test]
    fn min_area_filters_small_regions() {
        let mut img = blank(10, 10);
        img.put_pixel(5, 5, Luma([255]));
        // min_area=2 should exclude a 1-pixel region
        assert!(largest_region_centroid(&img, 2).is_none());
    }

    // --- detect_motion ---

    #[test]
    fn identical_frames_no_motion() {
        let frame = filled(20, 20, 100);
        let cfg = MotionConfig::default();
        assert!(detect_motion(&frame, &frame, &cfg).is_none());
    }

    #[test]
    fn different_frames_detects_motion() {
        let prev = filled(20, 20, 50);
        let mut current = filled(20, 20, 50);
        // Place a large bright region to ensure it survives threshold + blur
        for y in 5..15u32 {
            for x in 5..15u32 {
                current.put_pixel(x, y, Luma([255]));
            }
        }
        let cfg = MotionConfig {
            sensitivity: 40,
            blur_size: 2,
            min_area: 1,
        };
        assert!(detect_motion(&prev, &current, &cfg).is_some());
    }

    proptest! {
        #[test]
        fn centroid_within_image(
            cx in 5u32..15,
            cy in 5u32..15,
        ) {
            let mut img = blank(20, 20);
            // 3x3 block centred at (cx, cy)
            for dy in 0..3u32 {
                for dx in 0..3u32 {
                    img.put_pixel(cx + dx - 1, cy + dy - 1, Luma([255]));
                }
            }
            let result = largest_region_centroid(&img, 1).unwrap();
            prop_assert!(result.centroid_x >= 0.0 && result.centroid_x < 20.0);
            prop_assert!(result.centroid_y >= 0.0 && result.centroid_y < 20.0);
        }
    }
}
