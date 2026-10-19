// SPDX-License-Identifier: Apache-2.0

//! What the scan engine asks of a detector and embedder.
//!
//! The ONNX networks (`onnx_exec`) are one implementation. The synthetic
//! demo markers (`fixture`, a build feature) are the other. The engine does
//! not know which it has: gates, tracks, templates, and cards are the same.

use crate::geom::Rect;
use crate::quality::{self, Measured, Reject, FACE_COMPARE_PX};
use image::RgbImage;

pub const PERCEPTION_ONNX: &str = "onnx";
pub const PERCEPTION_FIDUCIAL: &str = "fiducial-v1";

/// One face on one frame, in original-frame pixels.
#[derive(Clone, Debug)]
pub struct Detection {
    pub rect: Rect,
    /// Short side of the box on the image the detector read.
    pub det_short: u32,
    pub score: f32,
    /// Eyes, nose, mouth corners, on the original frame.
    pub landmarks: Option<[(f32, f32); 5]>,
}

/// A face ready for the embedder.
#[derive(Clone, Debug)]
pub enum Aligned {
    /// The 112 px crop aligned to the ArcFace template.
    Face(RgbImage),
    /// A demo marker, read from the pixels.
    Marker { id: u16, rect: Rect, frame: u64 },
}

/// Milliseconds for one detector pass at a size, and one embedding.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub detect_ms: f64,
    pub embed_ms: f64,
}

pub trait Perception {
    /// `onnx` or `fiducial-v1`, as a poster pack names it.
    fn kind(&self) -> &'static str;
    /// The execution provider that registered, as ONNX Runtime names it.
    fn provider(&self) -> &str;
    /// The smallest face, in detection-image pixels, the detector reliably finds.
    fn min_face_px(&self) -> u32;
    /// Detections at or above this score are matched to tracks first.
    fn high_score(&self) -> f32;
    /// A face is compared only at or above this detector score.
    fn compare_score(&self) -> f32;
    /// Embedding length.
    fn dim(&self) -> usize;
    fn detect(&mut self, frame: &RgbImage, det_long: u32) -> Result<Vec<Detection>, String>;
    /// The hard quality gates.
    fn assess(&self, frame: &RgbImage, det: &Detection) -> Result<Measured, Reject> {
        quality::assess(frame, det.rect, det.score, self.compare_score(), det.landmarks.as_ref())
    }
    /// `None` when the face cannot be aligned. It is then not compared.
    fn align(&mut self, frame: &RgbImage, det: &Detection, index: u64) -> Result<Option<Aligned>, String>;
    /// A unit vector, or an error from the runtime.
    fn embed(&mut self, aligned: &Aligned) -> Result<Vec<f32>, String>;
    /// The demo markers use their own measured cutoff.
    fn cutoff(&self, bundle_threshold: f32) -> f32 {
        bundle_threshold
    }
    /// Time the networks on this device at `det_long`.
    fn time(&mut self, det_long: u32) -> Option<Timing>;
}

/// The detection size: the long side at which a face 112 px across in the
/// original reaches the detector at `min_face_px`, never above the original
/// and never below 640 px (or the original, when it is smaller).
pub fn auto_long_side(original_long: u32, min_face_px: u32) -> u32 {
    let original_long = original_long.max(1);
    let wanted = (u64::from(original_long) * u64::from(min_face_px)).div_ceil(u64::from(FACE_COMPARE_PX));
    let rounded = wanted.div_ceil(32) * 32;
    (rounded as u32).max(640).min(original_long)
}

/// A vector the cutoff can be applied to: the right length, finite, unit length.
pub fn valid_vector(v: &[f32], dim: usize) -> bool {
    if v.len() != dim || v.iter().any(|x| !x.is_finite()) {
        return false;
    }
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    (norm - 1.0).abs() < 1e-3
}

pub fn normalize(v: &mut [f32]) -> bool {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !(norm > 1e-6) || !norm.is_finite() {
        return false;
    }
    v.iter_mut().for_each(|x| *x /= norm);
    true
}

/// Cosine of two unit vectors of the same length. `None` when they are not.
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot.is_finite().then_some(dot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_112_px_face_reaches_the_detector_at_its_minimum() {
        // 4K: 3840 * 28 / 112 = 960.
        assert_eq!(auto_long_side(3840, 28), 960);
        // 12 megapixels.
        assert_eq!(auto_long_side(4032, 28), 1024);
        // 1080p would be 480, which is under the 640 floor.
        assert_eq!(auto_long_side(1920, 28), 640);
        // A small file is read at its own size, never enlarged.
        assert_eq!(auto_long_side(500, 28), 500);
        assert_eq!(auto_long_side(4000, 112), 4000);
        assert_eq!(auto_long_side(0, 28), 1);
    }

    #[test]
    fn vectors_are_checked_before_a_cutoff_is_applied() {
        let mut v = vec![3.0, 4.0];
        assert!(!valid_vector(&v, 2));
        assert!(normalize(&mut v));
        assert!(valid_vector(&v, 2));
        assert!(!valid_vector(&v, 3));
        assert!(!valid_vector(&[f32::NAN, 1.0], 2));
        assert!(!normalize(&mut [0.0, 0.0]));
        assert_eq!(cosine(&v, &v).map(|c| (c * 1000.0).round()), Some(1000.0));
        assert_eq!(cosine(&v, &[1.0]), None);
        assert_eq!(cosine(&[], &[]), None);
    }
}

/// One face on a still, as a scan sees it: its box, what the gates said, and
/// its vector when it passed them. For measuring a bundle (`eval/`); a scan
/// itself goes through the engine.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FaceVector {
    pub rect: Rect,
    pub det_short: u32,
    pub score: f32,
    /// Why the gates refused it, when they did.
    pub reject: Option<String>,
    pub embedding: Option<Vec<f32>>,
}

/// The faces on one still, through the same detection size, gates, alignment,
/// and embedder a scan uses.
pub fn face_vectors(perception: &mut dyn Perception, frame: &RgbImage) -> Result<Vec<FaceVector>, String> {
    let det_long = auto_long_side(frame.width().max(frame.height()), perception.min_face_px());
    let mut out = Vec::new();
    for det in perception.detect(frame, det_long)? {
        let mut face = FaceVector { rect: det.rect, det_short: det.det_short, score: det.score, reject: None, embedding: None };
        match perception.assess(frame, &det) {
            Err(reject) => face.reject = Some(reject.copy().0.into()),
            Ok(_) => match perception.align(frame, &det, 0)? {
                None => face.reject = Some(Reject::Unaligned.copy().0.into()),
                Some(aligned) => {
                    let v = perception.embed(&aligned)?;
                    if valid_vector(&v, perception.dim()) {
                        face.embedding = Some(v);
                    } else {
                        face.reject = Some("face_unscored".into());
                    }
                }
            },
        }
        out.push(face);
    }
    Ok(out)
}
