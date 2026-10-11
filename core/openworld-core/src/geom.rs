// SPDX-License-Identifier: Apache-2.0

use image::{Rgb, RgbImage};
use serde::{Deserialize, Serialize};

/// A face enters the inventory at this short side on the detection image.
pub const FACE_SEEN_PX: u32 = 64;
/// A face is compared only when the original-frame crop is at least this short side.
pub const FACE_COMPARE_PX: u32 = 112;
pub const PLATE_MIN_WIDTH: u32 = 80;
pub const PLATE_MIN_HEIGHT: u32 = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn short(self) -> u32 {
        self.w.min(self.h)
    }

    pub fn area(self) -> u64 {
        self.w as u64 * self.h as u64
    }

    pub fn intersection(self, other: Rect) -> Option<Rect> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self.x.saturating_add(self.w).min(other.x.saturating_add(other.w));
        let y1 = self.y.saturating_add(self.h).min(other.y.saturating_add(other.h));
        if x1 <= x0 || y1 <= y0 {
            None
        } else {
            Some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
        }
    }

    pub fn iou(self, other: Rect) -> f32 {
        let Some(inter) = self.intersection(other) else {
            return 0.0;
        };
        let union = self.area() + other.area() - inter.area();
        if union == 0 {
            0.0
        } else {
            inter.area() as f32 / union as f32
        }
    }

    pub fn clamp(self, width: u32, height: u32) -> Option<Rect> {
        if self.w == 0 || self.h == 0 || width == 0 || height == 0 {
            return None;
        }
        let x = self.x.min(width.saturating_sub(1));
        let y = self.y.min(height.saturating_sub(1));
        let w = self.w.min(width - x);
        let h = self.h.min(height - y);
        if w == 0 || h == 0 {
            None
        } else {
            Some(Rect { x, y, w, h })
        }
    }
}

/// How the detection image was produced from the original frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameMap {
    /// Multiply a detection-image x length by this to get original pixels.
    pub det_to_orig_x: f32,
    /// Multiply a detection-image y length by this to get original pixels.
    pub det_to_orig_y: f32,
}

impl FrameMap {
    pub fn identity() -> Self {
        Self { det_to_orig_x: 1.0, det_to_orig_y: 1.0 }
    }

    pub fn uniform(scale: f32) -> Self {
        Self { det_to_orig_x: scale, det_to_orig_y: scale }
    }

    pub fn to_original(self, det: Rect) -> Rect {
        Rect {
            x: (det.x as f32 * self.det_to_orig_x).round() as u32,
            y: (det.y as f32 * self.det_to_orig_y).round() as u32,
            w: (det.w as f32 * self.det_to_orig_x).round().max(1.0) as u32,
            h: (det.h as f32 * self.det_to_orig_y).round().max(1.0) as u32,
        }
    }
}

/// Resize so the long side is `long_side`. Never upscales.
/// Integer factors use a point sample so fixture markers stay aligned.
pub fn resize_long_side(src: &RgbImage, long_side: u32) -> (RgbImage, FrameMap) {
    let (w, h) = src.dimensions();
    let long = w.max(h);
    if long_side == 0 || long <= long_side || long_side >= long {
        return (src.clone(), FrameMap::identity());
    }
    if long % long_side == 0 {
        let factor = long / long_side;
        let nw = (w / factor).max(1);
        let nh = (h / factor).max(1);
        let mut dst = RgbImage::new(nw, nh);
        for y in 0..nh {
            for x in 0..nw {
                dst.put_pixel(x, y, *src.get_pixel(x * factor, y * factor));
            }
        }
        return (dst, FrameMap::uniform(factor as f32));
    }
    let scale = long_side as f32 / long as f32;
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    let mut dst = RgbImage::new(nw, nh);
    for y in 0..nh {
        for x in 0..nw {
            let sx = ((x as f32 + 0.5) / nw as f32 * w as f32).floor() as u32;
            let sy = ((y as f32 + 0.5) / nh as f32 * h as f32).floor() as u32;
            dst.put_pixel(x, y, *src.get_pixel(sx.min(w - 1), sy.min(h - 1)));
        }
    }
    // Each axis is rounded on its own. A tall frame's height scale is not its width scale,
    // and the short side of a wide face is that height.
    (
        dst,
        FrameMap {
            det_to_orig_x: w as f32 / nw as f32,
            det_to_orig_y: h as f32 / nh as f32,
        },
    )
}

pub fn crop(img: &RgbImage, rect: Rect) -> Option<RgbImage> {
    let rect = rect.clamp(img.width(), img.height())?;
    let mut out = RgbImage::new(rect.w, rect.h);
    for y in 0..rect.h {
        for x in 0..rect.w {
            out.put_pixel(x, y, *img.get_pixel(rect.x + x, rect.y + y));
        }
    }
    Some(out)
}

pub fn paint(canvas: &mut RgbImage, sprite: &RgbImage, x: u32, y: u32) {
    let (cw, ch) = canvas.dimensions();
    let (sw, sh) = sprite.dimensions();
    for yy in 0..sh {
        for xx in 0..sw {
            let dx = x + xx;
            let dy = y + yy;
            if dx < cw && dy < ch {
                canvas.put_pixel(dx, dy, *sprite.get_pixel(xx, yy));
            }
        }
    }
}

pub fn blank(w: u32, h: u32) -> RgbImage {
    RgbImage::from_pixel(w, h, Rgb([255, 255, 255]))
}

fn measured_step(fps: f64) -> f64 {
    if fps <= 5.0 { 1.0 } else { fps / 5.0 }
}

/// Frame indexes the detector will see.
/// `measured` is the frame on screen at each 5 Hz tick. `complete` is every frame.
/// Tick `k` is time `k/5` seconds, so the index is `floor(k * fps / 5)`.
pub fn sample_indices(frame_count: u64, fps: f64, measured: bool) -> Vec<u64> {
    if frame_count == 0 {
        return Vec::new();
    }
    if !measured {
        return (0..frame_count).collect();
    }
    let step = measured_step(fps);
    if !(step > 0.0) || !step.is_finite() {
        return vec![0];
    }
    let mut out = Vec::new();
    let mut tick = 0u64;
    while tick <= frame_count {
        let t = tick as f64 * step;
        if !t.is_finite() || t >= frame_count as f64 {
            break;
        }
        let idx = t.floor() as u64;
        if idx < frame_count && out.last().copied() != Some(idx) {
            out.push(idx);
        }
        tick += 1;
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

/// Whether frame `index` is on screen at the next 5 Hz tick.
/// `next_tick` starts at 0. A skipped index advances the clock, so one missing
/// frame does not stall the rest of the video.
pub fn take_measured_sample(index: u64, next_tick: &mut u64, fps: f64) -> bool {
    let step = measured_step(fps);
    if !(step > 0.0) || !step.is_finite() {
        return false;
    }
    let mut hops = 0u64;
    loop {
        let t = *next_tick as f64 * step;
        if !t.is_finite() {
            return false;
        }
        let want = t.floor() as u64;
        if index < want {
            return false;
        }
        if index == want {
            *next_tick = next_tick.saturating_add(1);
            let mut extra = 0u32;
            while extra < 8 {
                let later = *next_tick as f64 * step;
                if !later.is_finite() || later.floor() as u64 != index {
                    break;
                }
                *next_tick = next_tick.saturating_add(1);
                extra += 1;
            }
            return true;
        }
        *next_tick = next_tick.saturating_add(1);
        hops += 1;
        if hops > index.saturating_add(1) {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_four_on_a_640_preview_maps_to_a_large_original() {
        let mut src = blank(3840, 2160);
        src.put_pixel(600, 300, Rgb([0, 0, 0]));
        let (dst, map) = resize_long_side(&src, 640);
        assert_eq!(dst.dimensions(), (640, 360));
        assert_eq!(map.det_to_orig_x, 6.0);
        assert_eq!(map.det_to_orig_y, 6.0);
        let det = Rect { x: 100, y: 50, w: 64, h: 64 };
        let orig = map.to_original(det);
        assert_eq!(orig.w, 384);
        assert!(orig.short() >= FACE_COMPARE_PX);
        assert!(det.short() >= FACE_SEEN_PX);
    }

    #[test]
    fn a_wide_face_on_a_tall_frame_keeps_the_short_side() {
        // Height is the long side, so the two axes do not share one scale.
        // A wide face's short side is its height. Mapping that height by the
        // width scale reports 111 px and skips a comparison the original crop can take.
        let (_dst, map) = resize_long_side(&blank(241, 322), 320);
        let det = Rect { x: 0, y: 0, w: 120, h: 111 };
        let orig = map.to_original(det);
        assert!(
            orig.short() >= FACE_COMPARE_PX,
            "short side {} from {}x{}",
            orig.short(),
            orig.w,
            orig.h
        );
    }

    #[test]
    fn full_res_64_fails_the_compare_gate() {
        let det = Rect { x: 0, y: 0, w: 64, h: 64 };
        let orig = FrameMap::identity().to_original(det);
        assert!(det.short() >= FACE_SEEN_PX);
        assert!(orig.short() < FACE_COMPARE_PX);
    }

    fn kept(fps: f64, frames: u64) -> Vec<u64> {
        let mut tick = 0u64;
        (0..frames).filter(|&index| take_measured_sample(index, &mut tick, fps)).collect()
    }

    #[test]
    fn measured_is_five_frames_a_second() {
        let idx = sample_indices(90, 30.0, true);
        assert_eq!(idx, vec![0, 6, 12, 18, 24, 30, 36, 42, 48, 54, 60, 66, 72, 78, 84]);
        assert_eq!(sample_indices(5, 30.0, false).len(), 5);
    }

    #[test]
    fn film_rate_samples_the_frame_on_screen() {
        // 48 frames at 24 fps is two seconds. The frame on screen at each 5 Hz tick
        // is floor(k * 4.8), not the following frame the scan used to keep.
        let on_screen = vec![0, 4, 9, 14, 19, 24, 28, 33, 38, 43];
        assert_eq!(sample_indices(48, 24.0, true), on_screen);
        assert_eq!(kept(24.0, 48), on_screen);
        assert_ne!(kept(24.0, 48), vec![0, 5, 10, 15, 20, 24, 29, 34, 39, 44]);
        assert_eq!(kept(30.0, 90), sample_indices(90, 30.0, true));
        assert_eq!(kept(29.97, 60), sample_indices(60, 29.97, true));
        assert_eq!(kept(10.0, 20), sample_indices(20, 10.0, true));
        assert_eq!(kept(1.0, 8), sample_indices(8, 1.0, true));
        assert_eq!(kept(0.0, 4), sample_indices(4, 0.0, true));
    }

    #[test]
    fn a_missing_frame_does_not_stall_measured_sampling() {
        let mut tick = 0u64;
        assert!(take_measured_sample(0, &mut tick, 24.0));
        assert!(take_measured_sample(9, &mut tick, 24.0));
        assert!(!take_measured_sample(10, &mut tick, 24.0));
        assert!(take_measured_sample(14, &mut tick, 24.0));
    }
}
