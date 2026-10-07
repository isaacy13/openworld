// SPDX-License-Identifier: Apache-2.0

//! ArcFace crop. Five landmarks go to the 112 px template, then pixels are
//! `(channel - 127.5) / 127.5` in RGB, NCHW. This file does not contain weights.

use image::{Rgb, RgbImage};

/// InsightFace `arcface_dst` for a 112 px crop.
pub const TEMPLATE_112: [(f32, f32); 5] = [
    (38.2946, 51.6963),
    (73.5318, 51.5014),
    (56.0252, 71.7366),
    (41.5493, 92.3655),
    (70.7299, 92.2041),
];

pub const CROP: u32 = 112;

/// `[[a, b, tx], [c, d, ty]]` maps source pixels into the crop.
pub fn similarity(src: &[(f32, f32); 5], dst: &[(f32, f32); 5]) -> Option<[[f32; 3]; 2]> {
    let n = 5.0f32;
    let (mut mx, mut my) = (0.0f32, 0.0f32);
    let (mut dxm, mut dym) = (0.0f32, 0.0f32);
    for i in 0..5 {
        mx += src[i].0;
        my += src[i].1;
        dxm += dst[i].0;
        dym += dst[i].1;
    }
    mx /= n;
    my /= n;
    dxm /= n;
    dym /= n;
    let mut a = 0.0f32;
    let mut b = 0.0f32;
    let mut denom = 0.0f32;
    for i in 0..5 {
        let sx = src[i].0 - mx;
        let sy = src[i].1 - my;
        let dx = dst[i].0 - dxm;
        let dy = dst[i].1 - dym;
        a += sx * dx + sy * dy;
        b += sx * dy - sy * dx;
        denom += sx * sx + sy * sy;
    }
    if denom < 1e-6 {
        return None;
    }
    let cos_s = a / denom;
    let sin_s = b / denom;
    let tx = dxm - (cos_s * mx - sin_s * my);
    let ty = dym - (sin_s * mx + cos_s * my);
    Some([[cos_s, -sin_s, tx], [sin_s, cos_s, ty]])
}

/// Inverse of `similarity`: where a crop pixel reads from the source image.
fn inverse(m: [[f32; 3]; 2]) -> Option<[[f32; 3]; 2]> {
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if det.abs() < 1e-8 {
        return None;
    }
    let inv_det = 1.0 / det;
    let a = m[1][1] * inv_det;
    let b = -m[0][1] * inv_det;
    let c = -m[1][0] * inv_det;
    let d = m[0][0] * inv_det;
    let tx = -(a * m[0][2] + b * m[1][2]);
    let ty = -(c * m[0][2] + d * m[1][2]);
    Some([[a, b, tx], [c, d, ty]])
}

pub fn warp_112(image: &RgbImage, landmarks: &[(f32, f32); 5]) -> Option<RgbImage> {
    let forward = similarity(landmarks, &TEMPLATE_112)?;
    let back = inverse(forward)?;
    let mut out = RgbImage::new(CROP, CROP);
    let (w, h) = image.dimensions();
    for y in 0..CROP {
        for x in 0..CROP {
            let sx = back[0][0] * x as f32 + back[0][1] * y as f32 + back[0][2];
            let sy = back[1][0] * x as f32 + back[1][1] * y as f32 + back[1][2];
            out.put_pixel(x, y, sample(image, sx, sy, w, h));
        }
    }
    Some(out)
}

fn sample(image: &RgbImage, x: f32, y: f32, w: u32, h: u32) -> Rgb<u8> {
    if x < 0.0 || y < 0.0 || x >= w as f32 - 1.0 || y >= h as f32 - 1.0 {
        return Rgb([0, 0, 0]);
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p00 = image.get_pixel(x0, y0);
    let p10 = image.get_pixel(x1, y0);
    let p01 = image.get_pixel(x0, y1);
    let p11 = image.get_pixel(x1, y1);
    let mut px = [0u8; 3];
    for c in 0..3 {
        let v = (1.0 - fx) * (1.0 - fy) * p00[c] as f32
            + fx * (1.0 - fy) * p10[c] as f32
            + (1.0 - fx) * fy * p01[c] as f32
            + fx * fy * p11[c] as f32;
        px[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    Rgb(px)
}

/// RGB NCHW, `(pixel - 127.5) / 127.5`.
pub fn nchw(image: &RgbImage) -> Vec<f32> {
    let (w, h) = image.dimensions();
    let mut out = vec![0f32; (3 * w * h) as usize];
    let plane = (w * h) as usize;
    for y in 0..h {
        for x in 0..w {
            let p = image.get_pixel(x, y);
            let i = (y * w + x) as usize;
            for c in 0..3 {
                out[c * plane + i] = (p[c] as f32 - 127.5) / 127.5;
            }
        }
    }
    out
}

pub fn l2_normalize(v: &mut [f32]) {
    let mut sum = 0.0f32;
    for x in v.iter() {
        sum += x * x;
    }
    let norm = sum.sqrt();
    if norm > 1e-8 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_landmarks_are_the_identity() {
        let m = similarity(&TEMPLATE_112, &TEMPLATE_112).unwrap();
        assert!((m[0][0] - 1.0).abs() < 1e-4, "{m:?}");
        assert!(m[0][1].abs() < 1e-4);
        assert!(m[0][2].abs() < 1e-3);
        assert!(m[1][0].abs() < 1e-4);
        assert!((m[1][1] - 1.0).abs() < 1e-4);
        assert!(m[1][2].abs() < 1e-3);
    }

    #[test]
    fn normalization_is_unit_length() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        let n = (v[0] * v[0] + v[1] * v[1]).sqrt();
        assert!((n - 1.0).abs() < 1e-5);
    }
}
