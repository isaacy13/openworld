// SPDX-License-Identifier: Apache-2.0

//! Hard quality gates. A face that fails one is "Not compared", never cleared.
//!
//! The cutoff was measured on faces that passed these gates, so a face that
//! does not pass them is not scored against that cutoff at all. There is no
//! override. The limits are conservative: they drop faces that are clearly
//! too small, turned, blurred, or badly exposed, and keep the rest.

use crate::copy::{Copy, WHY_BLUR, WHY_CONFIDENCE, WHY_EXPOSURE, WHY_POSE, WHY_SMALL, WHY_UNALIGNED};
use crate::geom::Rect;
use image::imageops::{self, FilterType};
use image::RgbImage;

/// A face is compared only when the original-frame box is at least this short side.
pub const FACE_COMPARE_PX: u32 = 112;
/// Nose offset from the eye-mouth midline, in eye distances. About 35 degrees of turn.
const MAX_YAW: f32 = 0.40;
/// Nose height between the eyes (0) and the mouth (1). A frontal face is near 0.5.
const PITCH_RANGE: (f32, f32) = (0.20, 0.80);
/// Variance of the Laplacian on a 112 px gray face. Below this the face is a smear.
pub const MIN_SHARPNESS: f32 = 12.0;
/// Mean gray on the face, out of 255.
const EXPOSURE_RANGE: (f32, f32) = (35.0, 225.0);
/// Share of face pixels that are crushed to black or blown to white.
const MAX_CLIPPED: f32 = 0.35;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    Small,
    Unaligned,
    Confidence,
    Pose,
    Blur,
    Exposure,
}

impl Reject {
    pub fn copy(self) -> Copy {
        match self {
            Reject::Small => WHY_SMALL,
            Reject::Unaligned => WHY_UNALIGNED,
            Reject::Confidence => WHY_CONFIDENCE,
            Reject::Pose => WHY_POSE,
            Reject::Blur => WHY_BLUR,
            Reject::Exposure => WHY_EXPOSURE,
        }
    }
}

/// What the gates measured on one face. `rank` orders a track's frames: the
/// sharpest, most frontal, most confident frames are the ones embedded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measured {
    pub sharpness: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub brightness: f32,
    pub rank: f32,
}

/// The gates, in order. `landmarks` are on the original frame.
pub fn assess(
    frame: &RgbImage,
    rect: Rect,
    score: f32,
    min_score: f32,
    landmarks: Option<&[(f32, f32); 5]>,
) -> Result<Measured, Reject> {
    if rect.short() < FACE_COMPARE_PX {
        return Err(Reject::Small);
    }
    let Some(points) = landmarks else {
        return Err(Reject::Unaligned);
    };
    if !(score >= min_score) {
        return Err(Reject::Confidence);
    }
    let (yaw, pitch) = pose(points).ok_or(Reject::Unaligned)?;
    if yaw.abs() > MAX_YAW || pitch < PITCH_RANGE.0 || pitch > PITCH_RANGE.1 {
        return Err(Reject::Pose);
    }
    let gray = face_gray(frame, rect).ok_or(Reject::Unaligned)?;
    let (brightness, clipped) = exposure(&gray);
    if brightness < EXPOSURE_RANGE.0 || brightness > EXPOSURE_RANGE.1 || clipped > MAX_CLIPPED {
        return Err(Reject::Exposure);
    }
    let sharpness = sharpness(&gray);
    if !(sharpness >= MIN_SHARPNESS) {
        return Err(Reject::Blur);
    }
    let rank = score.clamp(0.0, 1.0) * (sharpness / 100.0).min(1.0) * (1.0 - yaw.abs());
    Ok(Measured { sharpness, yaw, pitch, brightness, rank })
}

/// Yaw and pitch from five points: eyes, nose, mouth corners. The face is
/// rotated so the eyes are level first, so a tilted head is not a turned one.
pub fn pose(points: &[(f32, f32); 5]) -> Option<(f32, f32)> {
    let [le, re, nose, ml, mr] = *points;
    let (dx, dy) = (re.0 - le.0, re.1 - le.1);
    let iod = (dx * dx + dy * dy).sqrt();
    if !(iod > 1.0) {
        return None;
    }
    let (c, s) = (dx / iod, dy / iod);
    // Rotate about the left eye so the eye line is the x axis.
    let level = |p: (f32, f32)| {
        let (x, y) = (p.0 - le.0, p.1 - le.1);
        (x * c + y * s, -x * s + y * c)
    };
    let (n, l, r) = (level(nose), level(ml), level(mr));
    let mouth = ((l.0 + r.0) / 2.0, (l.1 + r.1) / 2.0);
    let eye_mid_x = iod / 2.0;
    let midline_x = (eye_mid_x + mouth.0) / 2.0;
    let yaw = (n.0 - midline_x) / iod;
    if !(mouth.1 > 1.0) {
        return None;
    }
    let pitch = n.1 / mouth.1;
    (yaw.is_finite() && pitch.is_finite()).then_some((yaw, pitch))
}

/// The face box as a 112 px gray square, the inner part where the face is.
fn face_gray(frame: &RgbImage, rect: Rect) -> Option<image::GrayImage> {
    let rect = rect.clamp(frame.width(), frame.height())?;
    // The inner 80% of the box leaves out hair and background.
    let inset_x = rect.w / 10;
    let inset_y = rect.h / 10;
    let inner = Rect { x: rect.x + inset_x, y: rect.y + inset_y, w: rect.w - 2 * inset_x, h: rect.h - 2 * inset_y };
    if inner.w < 8 || inner.h < 8 {
        return None;
    }
    let view = imageops::crop_imm(frame, inner.x, inner.y, inner.w, inner.h).to_image();
    let small = imageops::resize(&view, 112, 112, FilterType::Triangle);
    Some(imageops::grayscale(&small))
}

fn exposure(gray: &image::GrayImage) -> (f32, f32) {
    let n = gray.len().max(1) as f32;
    let mut sum = 0u64;
    let mut clipped = 0u32;
    for p in gray.pixels() {
        let v = p[0];
        sum += u64::from(v);
        if v <= 8 || v >= 247 {
            clipped += 1;
        }
    }
    (sum as f32 / n, clipped as f32 / n)
}

/// Variance of the 4-neighbour Laplacian.
pub fn sharpness(gray: &image::GrayImage) -> f32 {
    let (w, h) = gray.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let at = |x: u32, y: u32| f32::from(gray.get_pixel(x, y)[0]);
    let mut sum = 0f64;
    let mut sq = 0f64;
    let mut n = 0f64;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let lap = 4.0 * at(x, y) - at(x - 1, y) - at(x + 1, y) - at(x, y - 1) - at(x, y + 1);
            sum += f64::from(lap);
            sq += f64::from(lap) * f64::from(lap);
            n += 1.0;
        }
    }
    let mean = sum / n;
    (sq / n - mean * mean) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arcface::TEMPLATE_112;
    use image::{Luma, Rgb};

    fn scaled(points: [(f32, f32); 5], k: f32, dx: f32) -> [(f32, f32); 5] {
        points.map(|(x, y)| (x * k + dx, y * k))
    }

    #[test]
    fn the_template_is_frontal() {
        let (yaw, pitch) = pose(&TEMPLATE_112).unwrap();
        assert!(yaw.abs() < 0.05, "{yaw}");
        assert!((pitch - 0.5).abs() < 0.05, "{pitch}");
    }

    #[test]
    fn a_turned_face_is_rejected_and_a_tilted_one_is_not() {
        let mut turned = TEMPLATE_112;
        turned[2].0 += 20.0;
        let (yaw, _) = pose(&turned).unwrap();
        assert!(yaw.abs() > MAX_YAW, "{yaw}");
        // Rotating every point does not change yaw or pitch.
        let (c, s) = (0.5f32.cos(), 0.5f32.sin());
        let tilted = TEMPLATE_112.map(|(x, y)| (x * c - y * s + 100.0, x * s + y * c + 100.0));
        let (yaw, pitch) = pose(&tilted).unwrap();
        assert!(yaw.abs() < 0.05 && (pitch - 0.5).abs() < 0.05, "{yaw} {pitch}");
        assert!(pose(&[(1.0, 1.0); 5]).is_none());
    }

    fn textured(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let v = (((x / 3) + (y / 3)) % 2 * 120 + 60) as u8;
            Rgb([v, v, v])
        })
    }

    #[test]
    fn gates_run_in_order() {
        let frame = textured(400, 400);
        let rect = Rect { x: 100, y: 100, w: 200, h: 200 };
        let points = scaled(TEMPLATE_112, 200.0 / 112.0, 100.0).map(|(x, y)| (x, y + 100.0));
        assert!(assess(&frame, rect, 0.9, 0.8, Some(&points)).is_ok());
        assert_eq!(assess(&frame, Rect { w: 100, ..rect }, 0.9, 0.8, Some(&points)), Err(Reject::Small));
        assert_eq!(assess(&frame, rect, 0.9, 0.8, None), Err(Reject::Unaligned));
        assert_eq!(assess(&frame, rect, 0.7, 0.8, Some(&points)), Err(Reject::Confidence));
        assert_eq!(assess(&frame, rect, f32::NAN, 0.8, Some(&points)), Err(Reject::Confidence));
        let flat = RgbImage::from_pixel(400, 400, Rgb([128, 128, 128]));
        assert_eq!(assess(&flat, rect, 0.9, 0.8, Some(&points)), Err(Reject::Blur));
        let dark = RgbImage::from_pixel(400, 400, Rgb([5, 5, 5]));
        assert_eq!(assess(&dark, rect, 0.9, 0.8, Some(&points)), Err(Reject::Exposure));
    }

    #[test]
    fn a_smeared_face_is_less_sharp_than_a_crisp_one() {
        let crisp = image::GrayImage::from_fn(112, 112, |x, y| Luma([if (x / 4 + y / 4) % 2 == 0 { 40 } else { 200 }]));
        let soft = imageops::blur(&crisp, 6.0);
        assert!(sharpness(&crisp) > MIN_SHARPNESS * 10.0);
        assert!(sharpness(&soft) < sharpness(&crisp) / 10.0);
    }
}
