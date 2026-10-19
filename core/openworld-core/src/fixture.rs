// SPDX-License-Identifier: Apache-2.0

//! Demo posters: synthetic markers, never faces. A build feature.
//!
//! A demo poster pack sets `perception` to `fiducial-v1`. The scan then reads
//! square markers from the pixels, and each marker's id stands for a person.
//! The vectors are not face features: a locked cosine still runs on them so
//! the demo exercises the same gates, tracks, templates, and cards as a real
//! scan. Their scores depend on where a marker sits, which lets repeated
//! trials cover a range. Nothing here is a claim about real faces.
//!
//! An official release build leaves this module out and refuses a demo pack.

use crate::arcface::TEMPLATE_112;
use crate::geom::{self, Rect};
use crate::perception::{Aligned, Detection, Perception, Timing, PERCEPTION_FIDUCIAL};
use crate::quality::{Measured, Reject, FACE_COMPARE_PX};
use image::{Rgb, RgbImage};

pub const EMBED_DIM: usize = 512;
/// The cutoff on demo cosines, whichever bundle was picked.
pub const FIXTURE_CUTOFF: f32 = 0.55;
/// Pixels per marker module in a demo scene. Eight modules across is 128 px,
/// which is over the 112 px compare size.
pub const SCENE_MODULE: u32 = 16;

/// Reads demo markers from the original frame.
pub struct Markers;

impl Perception for Markers {
    fn kind(&self) -> &'static str {
        PERCEPTION_FIDUCIAL
    }

    fn provider(&self) -> &str {
        "fixture"
    }

    fn min_face_px(&self) -> u32 {
        FACE_COMPARE_PX
    }

    fn high_score(&self) -> f32 {
        0.9
    }

    fn compare_score(&self) -> f32 {
        0.9
    }

    fn dim(&self) -> usize {
        EMBED_DIM
    }

    /// Markers are read on the original frame, so `det_long` is not used.
    fn detect(&mut self, frame: &RgbImage, _det_long: u32) -> Result<Vec<Detection>, String> {
        Ok(detect(frame)
            .into_iter()
            .map(|hit| Detection { rect: hit.rect, det_short: hit.rect.short(), score: 0.99, landmarks: Some(frontal(hit.rect)) })
            .collect())
    }

    /// A marker is square and frontal by construction. Only its size is gated.
    fn assess(&self, _frame: &RgbImage, det: &Detection) -> Result<Measured, Reject> {
        if det.rect.short() < FACE_COMPARE_PX {
            return Err(Reject::Small);
        }
        Ok(Measured { sharpness: 100.0, yaw: 0.0, pitch: 0.5, brightness: 128.0, rank: det.score })
    }

    fn align(&mut self, frame: &RgbImage, det: &Detection, index: u64) -> Result<Option<Aligned>, String> {
        Ok(geom::crop(frame, det.rect)
            .and_then(|crop| decode_marker(&crop))
            .map(|id| Aligned::Marker { id, rect: det.rect, frame: index }))
    }

    fn embed(&mut self, aligned: &Aligned) -> Result<Vec<f32>, String> {
        match aligned {
            Aligned::Marker { id, rect, frame } => Ok(probe(*id, rect.x, rect.y, rect.w, rect.h, *frame).0),
            Aligned::Face(_) => Err("A demo pack cannot score a face. Refusing.".into()),
        }
    }

    fn cutoff(&self, _bundle_threshold: f32) -> f32 {
        FIXTURE_CUTOFF
    }

    fn time(&mut self, _det_long: u32) -> Option<Timing> {
        Some(Timing { detect_ms: 2.0, embed_ms: 0.05 })
    }
}

/// The template's five points on a marker's box.
fn frontal(rect: Rect) -> [(f32, f32); 5] {
    let k = rect.w as f32 / 112.0;
    TEMPLATE_112.map(|(x, y)| (rect.x as f32 + x * k, rect.y as f32 + y * k))
}

#[derive(Clone, Copy)]
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn next_unit(&mut self) -> f32 {
        let n = (self.next() >> 40) as f32;
        n / (1u32 << 24) as f32 - 0.5
    }
}

/// A demo poster's vector for marker `id`.
pub fn unit_embedding(id: u16) -> Vec<f32> {
    let mut rng = SplitMix(0x0EED_F00D ^ u64::from(id).wrapping_mul(0x9E37));
    let mut v: Vec<f32> = (0..EMBED_DIM).map(|_| rng.next_unit()).collect();
    crate::perception::normalize(&mut v);
    v
}

/// A probe for one marker sighting, and its cosine with the clean poster vector.
pub fn probe(id: u16, x: u32, y: u32, w: u32, h: u32, frame: u64) -> (Vec<f32>, f32) {
    let base = unit_embedding(id);
    let salt = placement_salt(x, y, w, h, frame);
    let alpha = (salt % 100) as f32 / 99.0 * 0.85;
    let mut rng = SplitMix(0x51A7 ^ u64::from(salt));
    let mut noise: Vec<f32> = (0..EMBED_DIM).map(|_| rng.next_unit()).collect();
    let dot: f32 = base.iter().zip(&noise).map(|(a, b)| a * b).sum();
    for (n, b) in noise.iter_mut().zip(&base) {
        *n -= dot * b;
    }
    crate::perception::normalize(&mut noise);
    let mut out: Vec<f32> = base.iter().zip(&noise).map(|(b, n)| (1.0 - alpha) * b + alpha * n).collect();
    crate::perception::normalize(&mut out);
    let score = out.iter().zip(&base).map(|(a, b)| a * b).sum();
    (out, score)
}

pub fn placement_salt(x: u32, y: u32, w: u32, h: u32, frame: u64) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for v in [x, y, w, h, frame as u32, (frame >> 32) as u32] {
        hash ^= v;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

/// A marker found on a picture.
#[derive(Clone, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub id: u16,
}

/// A marker for `id`, eight modules across.
pub fn render(id: u16, module: u32) -> RgbImage {
    let mut bits = [false; 16];
    let packed = pack(id);
    // 4x4 row-major. (0,0) is always black and (3,3) is always white.
    bits[0] = true;
    bits[15] = false;
    for (i, (r, c)) in DATA_CELLS.iter().enumerate() {
        bits[(r * 4 + c) as usize] = ((packed >> (13 - i)) & 1) == 1;
    }
    let m = module.max(1);
    let mut img = RgbImage::from_pixel(8 * m, 8 * m, Rgb([255, 255, 255]));
    for r in 0..8 {
        for c in 0..8 {
            let border = r == 0 || r == 7 || c == 0 || c == 7;
            let ring = !border && (r == 1 || r == 6 || c == 1 || c == 6);
            let black = border || (!ring && bits[((r - 2) * 4 + (c - 2)) as usize]);
            let color = if black { Rgb([0, 0, 0]) } else { Rgb([255, 255, 255]) };
            for yy in 0..m {
                for xx in 0..m {
                    img.put_pixel(c * m + xx, r * m + yy, color);
                }
            }
        }
    }
    img
}

pub fn detect(img: &RgbImage) -> Vec<Hit> {
    let (w, h) = img.dimensions();
    if w < 16 || h < 16 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let mut y = 0u32;
    while y + 8 < h {
        let mut x = 0u32;
        while x + 8 < w {
            if is_black(img, x, y) && (x == 0 || !is_black(img, x - 1, y)) && (y == 0 || !is_black(img, x, y - 1)) {
                if let Some(hit) = try_at(img, x, y) {
                    x = x.saturating_add((hit.rect.w / 2).max(1));
                    hits.push(hit);
                    continue;
                }
            }
            x += 1;
        }
        y += 1;
    }
    hits
}

/// The id of the marker that fills `crop`.
pub fn decode_marker(crop: &RgbImage) -> Option<u16> {
    if let Some(hit) = detect(crop).into_iter().find(|hit| hit.rect.x <= 2 && hit.rect.y <= 2) {
        return Some(hit.id);
    }
    try_at(crop, 0, 0).map(|hit| hit.id)
}

pub fn place(canvas: &mut RgbImage, sprite: &RgbImage, x: u32, y: u32) {
    geom::paint(canvas, sprite, x, y);
}

const DATA_CELLS: [(u32, u32); 14] =
    [(0, 1), (0, 2), (0, 3), (1, 0), (1, 1), (1, 2), (1, 3), (2, 0), (2, 1), (2, 2), (2, 3), (3, 0), (3, 1), (3, 2)];

fn pack(id: u16) -> u16 {
    let id = id & 0x03FF;
    let sum = (id ^ (id >> 4) ^ (id >> 8)) & 0x0F;
    (id << 4) | sum
}

fn unpack(packed: u16) -> Option<u16> {
    let id = (packed >> 4) & 0x03FF;
    let sum = packed & 0x0F;
    ((id ^ (id >> 4) ^ (id >> 8)) & 0x0F == sum).then_some(id)
}

fn try_at(img: &RgbImage, x: u32, y: u32) -> Option<Hit> {
    let side = black_run(img, x, y, 1, 0);
    if side < 16 || side % 8 != 0 || x + side > img.width() || y + side > img.height() {
        return None;
    }
    if black_run(img, x, y, 0, 1) != side || !square_frame(img, x, y, side) {
        return None;
    }
    let module = side / 8;
    let mut bits = [false; 16];
    for r in 0..4 {
        for c in 0..4 {
            bits[(r * 4 + c) as usize] = sample(img, x, y, module, r + 2, c + 2)?;
        }
    }
    if !bits[0] || bits[15] {
        return None;
    }
    let mut packed = 0u16;
    for (i, (r, c)) in DATA_CELLS.iter().enumerate() {
        packed |= u16::from(bits[(r * 4 + c) as usize]) << (13 - i);
    }
    Some(Hit { rect: Rect { x, y, w: side, h: side }, id: unpack(packed)? })
}

fn square_frame(img: &RgbImage, x: u32, y: u32, side: u32) -> bool {
    let module = side / 8;
    (0..8).all(|i| {
        let edges = [(0, i), (7, i), (i, 0), (i, 7)].iter().all(|&(r, c)| sample(img, x, y, module, r, c) == Some(true));
        let ring = !(1..7).contains(&i)
            || [(1, i), (6, i), (i, 1), (i, 6)].iter().all(|&(r, c)| sample(img, x, y, module, r, c) == Some(false));
        edges && ring
    })
}

fn sample(img: &RgbImage, x: u32, y: u32, module: u32, r: u32, c: u32) -> Option<bool> {
    let px = x + c * module + module / 2;
    let py = y + r * module + module / 2;
    if px >= img.width() || py >= img.height() {
        return None;
    }
    let p = img.get_pixel(px, py);
    let luma = (u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2])) / 3;
    if luma < 96 {
        Some(true)
    } else if luma > 160 {
        Some(false)
    } else {
        None
    }
}

fn black_run(img: &RgbImage, x: u32, y: u32, dx: u32, dy: u32) -> u32 {
    let (w, h) = img.dimensions();
    let (mut cx, mut cy, mut n) = (x, y, 0u32);
    while cx < w && cy < h && is_black(img, cx, cy) && n <= 4096 {
        n += 1;
        cx += dx;
        cy += dy;
    }
    n
}

fn is_black(img: &RgbImage, x: u32, y: u32) -> bool {
    let p = img.get_pixel(x, y);
    p[0] < 96 && p[1] < 96 && p[2] < 96
}

/// A synthetic still: one marker large enough to compare, for poster `compared_id`,
/// and one marker that is only seen. No photograph of a person.
pub struct DemoLayout {
    pub image: RgbImage,
    pub compared_id: u16,
}

pub fn demo_scene() -> DemoLayout {
    let compared_id = 7u16;
    let marker = render(compared_id, SCENE_MODULE);
    let (mw, mh) = marker.dimensions();
    let (x, y) = passing_origin(compared_id, mw, mh);
    let mut image = geom::blank(640, 480);
    place(&mut image, &marker, x, y);
    // Eight modules of eight pixels is 64 px: seen, under the compare size.
    place(&mut image, &render(11, 8), 420, 36);
    DemoLayout { image, compared_id }
}

/// A place where the marker for `id` scores over the demo cutoff on frame 0.
pub fn passing_origin(id: u16, w: u32, h: u32) -> (u32, u32) {
    let mut fallback = (16u32, 16u32);
    for y in (12..70).step_by(2) {
        for x in (12..70).step_by(2) {
            fallback = (x, y);
            if probe(id, x, y, w, h, 0).1 >= FIXTURE_CUTOFF {
                return (x, y);
            }
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_round_trips() {
        let marker = render(42, 8);
        assert_eq!(marker.width(), 64);
        let mut canvas = geom::blank(200, 160);
        place(&mut canvas, &marker, 20, 24);
        let hits = detect(&canvas);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, 42);
        assert_eq!(hits[0].rect, Rect { x: 20, y: 24, w: 64, h: 64 });
        assert_eq!(decode_marker(&marker), Some(42));
    }

    #[test]
    fn identical_posters_score_one_and_probes_mix() {
        let a = unit_embedding(7);
        let dot: f32 = a.iter().map(|x| x * x).sum();
        assert!((dot - 1.0).abs() < 1e-5);
        let (_probe, score) = probe(7, 20, 24, 128, 128, 0);
        assert!(score > 0.1 && score <= 1.0);
    }

    #[test]
    fn the_demo_scene_compares_one_marker_and_sees_another() {
        let scene = demo_scene();
        let hits = detect(&scene.image);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.id == 7 && h.rect.short() >= FACE_COMPARE_PX));
        assert!(hits.iter().any(|h| h.id == 11 && h.rect.short() < FACE_COMPARE_PX));
    }
}
