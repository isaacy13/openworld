// SPDX-License-Identifier: Apache-2.0

//! Synthetic markers for the fixture pipeline.
//!
//! These are not faces and not a face detector. SCRFD weights are research-only
//! and are not bundled. A fixture poster pack sets `perception` to `fiducial-v1`,
//! and the scan reads these markers from pixels.

use crate::geom::{paint, Rect};
use image::{Rgb, RgbImage};

pub const PERCEPTION_FIDUCIAL: &str = "fiducial-v1";
pub const PERCEPTION_ONNX: &str = "onnx";

const PLATE_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Marker {
    Face { id: u16 },
    Vehicle,
    Plate,
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub marker: Marker,
    pub score: f32,
    /// Five points on the detection image, when the detector produced them.
    pub landmarks: Option<[(f32, f32); 5]>,
}

pub fn render_face(id: u16) -> RgbImage {
    render_face_module(id, 8)
}

pub fn render_face_module(id: u16, module: u32) -> RgbImage {
    let mut bits = [false; 16];
    let packed = pack_face(id);
    // 4x4 row-major. (0,0) is always black and (3,3) is always white.
    bits[0] = true;
    bits[15] = false;
    for (i, (r, c)) in face_data_cells().iter().enumerate() {
        bits[(r * 4 + c) as usize] = ((packed >> (13 - i)) & 1) == 1;
    }
    render_square(&bits, module.max(1))
}

pub fn render_vehicle(module: u32) -> RgbImage {
    let mut bits = [false; 16];
    bits[0] = false;
    bits[15] = true;
    let magic: u16 = 0b1100_1010_0101_11;
    let cells = face_data_cells();
    for (i, (r, c)) in cells.iter().enumerate() {
        bits[(r * 4 + c) as usize] = ((magic >> (13 - i)) & 1) == 1;
    }
    scale_nn(&render_square(&bits, 1), module)
}

pub fn render_plate(text: &str, module: u32) -> Option<RgbImage> {
    let bits = pack_plate(text)?;
    let m = module.max(1);
    let w = 16 * m;
    let h = 8 * m;
    let mut img = RgbImage::from_pixel(w, h, Rgb([255, 255, 255]));
    for r in 0..8 {
        for c in 0..16 {
            let border = r == 0 || r == 7 || c == 0 || c == 15;
            let ring = r == 1 || r == 6 || c == 1 || c == 14;
            let color = if border {
                Rgb([0, 0, 0])
            } else if ring {
                Rgb([255, 255, 255])
            } else {
                let dr = r - 2;
                let dc = c - 2;
                let idx = dr * 12 + dc;
                if bits[idx as usize] {
                    Rgb([0, 0, 0])
                } else {
                    Rgb([255, 255, 255])
                }
            };
            fill_module(&mut img, c, r, m, color);
        }
    }
    Some(img)
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
            if is_black(img, x, y) && left_is_clear(img, x, y) && above_is_clear(img, x, y) {
                if let Some(hit) = try_at(img, x, y) {
                    let skip = hit.rect.w / 2;
                    hits.push(hit);
                    x = x.saturating_add(skip.max(1));
                    continue;
                }
            }
            x += 1;
        }
        y += 1;
    }
    hits
}

pub fn decode_face(crop: &RgbImage) -> Option<u16> {
    let hits = detect(crop);
    for hit in hits {
        if let Marker::Face { id } = hit.marker {
            if hit.rect.x <= 2 && hit.rect.y <= 2 {
                return Some(id);
            }
        }
    }
    // A tight crop is the marker itself, so the corner is (0, 0).
    match try_at(crop, 0, 0)? {
        Hit { marker: Marker::Face { id }, .. } => Some(id),
        _ => None,
    }
}

pub fn ocr_plate(crop: &RgbImage) -> Option<String> {
    if let Some(text) = read_plate_at(crop, 0, 0, crop.width(), crop.height()) {
        return Some(text);
    }
    for hit in detect(crop) {
        if hit.marker == Marker::Plate {
            return read_plate_at(crop, hit.rect.x, hit.rect.y, hit.rect.w, hit.rect.h);
        }
    }
    None
}

fn face_data_cells() -> [(u32, u32); 14] {
    [
        (0, 1),
        (0, 2),
        (0, 3),
        (1, 0),
        (1, 1),
        (1, 2),
        (1, 3),
        (2, 0),
        (2, 1),
        (2, 2),
        (2, 3),
        (3, 0),
        (3, 1),
        (3, 2),
    ]
}

fn pack_face(id: u16) -> u16 {
    let id = id & 0x03FF;
    let sum = ((id ^ (id >> 4) ^ (id >> 8)) & 0x0F) as u16;
    (id << 4) | sum
}

fn unpack_face(packed: u16) -> Option<u16> {
    let id = (packed >> 4) & 0x03FF;
    let sum = packed & 0x0F;
    let expect = (id ^ (id >> 4) ^ (id >> 8)) & 0x0F;
    if sum == expect {
        Some(id)
    } else {
        None
    }
}

fn pack_plate(text: &str) -> Option<[bool; 48]> {
    let bytes = text.as_bytes();
    if bytes.len() != 6 {
        return None;
    }
    let mut symbols = [0u8; 6];
    for (i, b) in bytes.iter().enumerate() {
        let up = b.to_ascii_uppercase();
        let pos = PLATE_ALPHABET.iter().position(|c| *c == up)?;
        symbols[i] = pos as u8;
    }
    let mut checksum = 0x3Cu8;
    for s in symbols {
        checksum ^= s;
    }
    let mut raw = [0u8; 48];
    let mut bit_i = 0;
    for s in symbols {
        for k in (0..6).rev() {
            raw[bit_i] = (s >> k) & 1;
            bit_i += 1;
        }
    }
    for k in (0..8).rev() {
        raw[bit_i] = (checksum >> k) & 1;
        bit_i += 1;
    }
    let mut bits = [false; 48];
    for (i, b) in raw.iter().enumerate() {
        bits[i] = *b == 1;
    }
    Some(bits)
}

fn unpack_plate(bits: &[bool; 48]) -> Option<String> {
    let mut symbols = [0u8; 6];
    let mut bit_i = 0;
    for s in &mut symbols {
        let mut v = 0u8;
        for _ in 0..6 {
            v = (v << 1) | u8::from(bits[bit_i]);
            bit_i += 1;
        }
        if v as usize >= PLATE_ALPHABET.len() {
            return None;
        }
        *s = v;
    }
    let mut checksum = 0u8;
    for _ in 0..8 {
        checksum = (checksum << 1) | u8::from(bits[bit_i]);
        bit_i += 1;
    }
    let mut expect = 0x3Cu8;
    for s in symbols {
        expect ^= s;
    }
    if checksum != expect {
        return None;
    }
    let mut text = String::new();
    for s in symbols {
        text.push(PLATE_ALPHABET[s as usize] as char);
    }
    Some(text)
}

fn render_square(data4: &[bool; 16], module: u32) -> RgbImage {
    let m = module.max(1);
    let side = 8 * m;
    let mut img = RgbImage::from_pixel(side, side, Rgb([255, 255, 255]));
    for r in 0..8 {
        for c in 0..8 {
            let border = r == 0 || r == 7 || c == 0 || c == 7;
            let ring = !border && (r == 1 || r == 6 || c == 1 || c == 6);
            let color = if border {
                Rgb([0, 0, 0])
            } else if ring {
                Rgb([255, 255, 255])
            } else {
                let dr = r - 2;
                let dc = c - 2;
                if data4[(dr * 4 + dc) as usize] {
                    Rgb([0, 0, 0])
                } else {
                    Rgb([255, 255, 255])
                }
            };
            fill_module(&mut img, c, r, m, color);
        }
    }
    img
}

fn fill_module(img: &mut RgbImage, c: u32, r: u32, m: u32, color: Rgb<u8>) {
    for yy in 0..m {
        for xx in 0..m {
            img.put_pixel(c * m + xx, r * m + yy, color);
        }
    }
}

fn scale_nn(src: &RgbImage, factor: u32) -> RgbImage {
    let factor = factor.max(1);
    if factor == 1 {
        return src.clone();
    }
    let (w, h) = src.dimensions();
    let mut dst = RgbImage::new(w * factor, h * factor);
    for y in 0..h {
        for x in 0..w {
            let p = *src.get_pixel(x, y);
            for yy in 0..factor {
                for xx in 0..factor {
                    dst.put_pixel(x * factor + xx, y * factor + yy, p);
                }
            }
        }
    }
    dst
}

fn try_at(img: &RgbImage, x: u32, y: u32) -> Option<Hit> {
    let (w, h) = img.dimensions();
    let run = black_run(img, x, y, 1, 0)?;
    if run < 16 || x + run > w || y + 8 > h {
        return None;
    }
    if run % 8 == 0 {
        let side = run;
        let down = black_run(img, x, y, 0, 1)?;
        if down == side && square_frame(img, x, y, side) {
            return classify_square(img, x, y, side);
        }
    }
    if run % 16 == 0 {
        let width = run;
        let module = width / 16;
        let height = module * 8;
        if y + height <= h && plate_frame(img, x, y, width, height) {
            return Some(Hit {
                rect: Rect { x, y, w: width, h: height },
                marker: Marker::Plate,
                score: 0.99,
                landmarks: None,
            });
        }
    }
    None
}

fn classify_square(img: &RgbImage, x: u32, y: u32, side: u32) -> Option<Hit> {
    let module = side / 8;
    let mut bits = [false; 16];
    for r in 0..4 {
        for c in 0..4 {
            bits[(r * 4 + c) as usize] = sample_module(img, x, y, module, r + 2, c + 2)?;
        }
    }
    let rect = Rect { x, y, w: side, h: side };
    if bits[0] && !bits[15] {
        let mut packed = 0u16;
        for (i, (r, c)) in face_data_cells().iter().enumerate() {
            let bit = u16::from(bits[(r * 4 + c) as usize]);
            packed |= bit << (13 - i);
        }
        let id = unpack_face(packed)?;
        return Some(Hit { rect, marker: Marker::Face { id }, score: 0.99, landmarks: None });
    }
    if !bits[0] && bits[15] {
        let mut packed = 0u16;
        for (i, (r, c)) in face_data_cells().iter().enumerate() {
            let bit = u16::from(bits[(r * 4 + c) as usize]);
            packed |= bit << (13 - i);
        }
        if packed == 0b1100_1010_0101_11 {
            return Some(Hit { rect, marker: Marker::Vehicle, score: 0.99, landmarks: None });
        }
    }
    None
}

fn read_plate_at(img: &RgbImage, x: u32, y: u32, width: u32, height: u32) -> Option<String> {
    if width < 16 || height < 8 || width % 16 != 0 || height % 8 != 0 {
        return None;
    }
    if width / 16 != height / 8 {
        return None;
    }
    if !plate_frame(img, x, y, width, height) {
        return None;
    }
    let module = width / 16;
    let mut bits = [false; 48];
    for r in 0..4 {
        for c in 0..12 {
            bits[(r * 12 + c) as usize] = sample_module(img, x, y, module, r + 2, c + 2)?;
        }
    }
    unpack_plate(&bits)
}

fn square_frame(img: &RgbImage, x: u32, y: u32, side: u32) -> bool {
    let module = side / 8;
    if module == 0 {
        return false;
    }
    for i in 0..8 {
        if sample_module(img, x, y, module, 0, i) != Some(true) {
            return false;
        }
        if sample_module(img, x, y, module, 7, i) != Some(true) {
            return false;
        }
        if sample_module(img, x, y, module, i, 0) != Some(true) {
            return false;
        }
        if sample_module(img, x, y, module, i, 7) != Some(true) {
            return false;
        }
        if (1..7).contains(&i) {
            if sample_module(img, x, y, module, 1, i) != Some(false) {
                return false;
            }
            if sample_module(img, x, y, module, 6, i) != Some(false) {
                return false;
            }
            if sample_module(img, x, y, module, i, 1) != Some(false) {
                return false;
            }
            if sample_module(img, x, y, module, i, 6) != Some(false) {
                return false;
            }
        }
    }
    true
}

fn plate_frame(img: &RgbImage, x: u32, y: u32, width: u32, height: u32) -> bool {
    let module = width / 16;
    if module == 0 || height / 8 != module {
        return false;
    }
    for c in 0..16 {
        if sample_module(img, x, y, module, 0, c) != Some(true) {
            return false;
        }
        if sample_module(img, x, y, module, 7, c) != Some(true) {
            return false;
        }
    }
    for r in 0..8 {
        if sample_module(img, x, y, module, r, 0) != Some(true) {
            return false;
        }
        if sample_module(img, x, y, module, r, 15) != Some(true) {
            return false;
        }
    }
    for c in 1..15 {
        if sample_module(img, x, y, module, 1, c) != Some(false) {
            return false;
        }
        if sample_module(img, x, y, module, 6, c) != Some(false) {
            return false;
        }
    }
    true
}

fn sample_module(img: &RgbImage, x: u32, y: u32, module: u32, r: u32, c: u32) -> Option<bool> {
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

fn black_run(img: &RgbImage, x: u32, y: u32, dx: u32, dy: u32) -> Option<u32> {
    let (w, h) = img.dimensions();
    let mut n = 0u32;
    let mut cx = x;
    let mut cy = y;
    while cx < w && cy < h && is_black(img, cx, cy) {
        n += 1;
        if n > 4096 {
            break;
        }
        cx += dx;
        cy += dy;
        if dx == 0 && dy == 0 {
            break;
        }
    }
    if n == 0 {
        None
    } else {
        Some(n)
    }
}

fn is_black(img: &RgbImage, x: u32, y: u32) -> bool {
    let p = img.get_pixel(x, y);
    p[0] < 96 && p[1] < 96 && p[2] < 96
}

fn left_is_clear(img: &RgbImage, x: u32, y: u32) -> bool {
    x == 0 || !is_black(img, x - 1, y)
}

fn above_is_clear(img: &RgbImage, x: u32, y: u32) -> bool {
    y == 0 || !is_black(img, x, y - 1)
}

pub fn place(canvas: &mut RgbImage, sprite: &RgbImage, x: u32, y: u32) {
    paint(canvas, sprite, x, y);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::blank;

    #[test]
    fn face_roundtrip() {
        let marker = render_face_module(42, 8);
        assert_eq!(marker.width(), 64);
        let mut canvas = blank(200, 160);
        place(&mut canvas, &marker, 20, 24);
        let hits = detect(&canvas);
        assert!(hits.iter().any(|h| h.marker == Marker::Face { id: 42 }));
    }

    #[test]
    fn plate_and_vehicle_roundtrip() {
        let plate = render_plate("FIX123", 8).unwrap();
        let vehicle = render_vehicle(8);
        let mut canvas = blank(400, 240);
        place(&mut canvas, &plate, 16, 16);
        place(&mut canvas, &vehicle, 240, 80);
        let hits = detect(&canvas);
        assert!(hits.iter().any(|h| h.marker == Marker::Plate));
        assert!(hits.iter().any(|h| h.marker == Marker::Vehicle));
        let text = ocr_plate(&plate).unwrap();
        assert_eq!(text, "FIX123");
    }
}
