// SPDX-License-Identifier: Apache-2.0

//! ONNX Runtime sessions for a bundle whose weights are pinned and present.
//!
//! Each session is committed from the exact bytes whose SHA-256 was checked.
//! A session that does not load, or whose outputs are not SCRFD, YuNet,
//! ArcFace, or SFace, refuses the scan. An empty detection list is never
//! produced in that case.
//!
//! A detector exported with a fixed square input reads the detection image
//! letterboxed into it; a larger image is read twice, shrunk whole for large
//! faces and in overlapping tiles for small ones. A detector with a dynamic
//! input reads the detection image once, at its own size.

use crate::arcface::{self, CROP};
use crate::bundle::Bundle;
use crate::geom::{self, FrameMap, Rect};
use crate::perception::{Aligned, Detection, Perception, Timing, PERCEPTION_ONNX};
use crate::scrfd::{self, nms, FaceBox, Letterbox};
use crate::yunet;
use image::imageops::{self, FilterType};
use image::RgbImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;
use std::time::Instant;

/// SCRFD keeps a box at this score. Its outputs are already calibrated low.
const SCRFD_SCORE: f32 = 0.3;
/// YuNet keeps a box at this score. OpenCV's sample uses 0.9; the scan wants
/// fewer misses in the inventory, and the compare gate is stricter.
pub const YUNET_SCORE: f32 = 0.6;
/// Tiles share this many pixels, so a face cut by one tile is whole in the next.
const TILE_OVERLAP: u32 = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetectorKind {
    /// RGB, `(pixel - 127.5) / 128`, six or nine outputs.
    Scrfd,
    /// BGR, raw pixels, twelve outputs named by stride.
    Yunet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbedderKind {
    /// RGB, `(pixel - 127.5) / 127.5`.
    ArcFace,
    /// RGB, raw pixels. That is how OpenCV's `FaceRecognizerSF` feeds it.
    SFace,
}

pub struct FaceModels {
    detector: Session,
    detector_kind: DetectorKind,
    /// The square the detector was exported with. `None` is a dynamic input.
    side: Option<u32>,
    embedder: Session,
    embedder_kind: EmbedderKind,
    dim: usize,
    provider: &'static str,
}

/// Load the bundle's detector and embedder from the bytes the catalog verified.
pub fn load(bundle: &Bundle, accelerate: bool) -> Result<FaceModels, String> {
    let detector_bytes = bundle.verified_bytes("detector").map_err(|err| err.text)?;
    let embedder_bytes = bundle.verified_bytes("embedder").map_err(|err| err.text)?;
    let (mut detector, provider) = crate::runtime::session(&detector_bytes, accelerate)
        .map_err(|err| format!("The detector did not load. Refusing. {err}"))?;
    // The cutoff was measured on CPU embeddings, and a provider that drifts would
    // move scores near it. Five embeddings a face cost little on the CPU.
    let (mut embedder, _) = crate::runtime::session(&embedder_bytes, false)
        .map_err(|err| format!("The embedder did not load. Refusing. {err}"))?;
    let side = square_side(&detector);
    let detector_kind = check_detector(&mut detector, side.unwrap_or(640))?;
    let embedder_kind = if bundle.embedder.to_ascii_lowercase().starts_with("sface") {
        EmbedderKind::SFace
    } else {
        EmbedderKind::ArcFace
    };
    let dim = check_embedder(&mut embedder)?;
    Ok(FaceModels { detector, detector_kind, side, embedder, embedder_kind, dim, provider })
}

/// The square input a model was exported with, when its shape says so.
fn square_side(session: &Session) -> Option<u32> {
    let shape = session.inputs().first()?.dtype().tensor_shape()?;
    if shape.len() == 4 && shape[2] > 0 && shape[2] == shape[3] && shape[2] <= 4096 {
        Some(shape[2] as u32)
    } else {
        None
    }
}

fn check_detector(session: &mut Session, side: u32) -> Result<DetectorKind, String> {
    let outputs = run_zeros(session, side, side)?;
    match outputs.len() {
        6 | 9 if scrfd_shapes_fit(&outputs, side) => Ok(DetectorKind::Scrfd),
        12 if yunet_planes(&outputs).is_some() => Ok(DetectorKind::Yunet),
        _ => Err("The detector output was not recognized. Refusing.".into()),
    }
}

/// SCRFD's planes are scores, then boxes, then landmarks, stride 8 first. An
/// export in another order would decode to nothing and read as "no faces".
fn scrfd_shapes_fit(outputs: &[(String, Vec<f32>)], side: u32) -> bool {
    scrfd::STRIDES.iter().enumerate().all(|(level, stride)| {
        let cells = ((side / stride) * (side / stride) * scrfd::ANCHORS) as usize;
        outputs[level].1.len() == cells
            && outputs[level + 3].1.len() == cells * 4
            && (outputs.len() == 6 || outputs[level + 6].1.len() == cells * 10)
    })
}

fn check_embedder(session: &mut Session) -> Result<usize, String> {
    let outputs = run_zeros(session, CROP, CROP)?;
    match outputs.first() {
        Some((_, first)) if first.len() >= 128 && first.len() <= 4096 => Ok(first.len()),
        _ => Err("The embedder output was not recognized. Refusing.".into()),
    }
}

fn run_zeros(session: &mut Session, w: u32, h: u32) -> Result<Vec<(String, Vec<f32>)>, String> {
    let input = Array4::<f32>::zeros((1, 3, h as usize, w as usize));
    run(session, &input)
}

fn run(session: &mut Session, input: &Array4<f32>) -> Result<Vec<(String, Vec<f32>)>, String> {
    let outputs = session
        .run(ort::inputs![TensorRef::from_array_view(input).map_err(|err| err.to_string())?])
        .map_err(|err| err.to_string())?;
    let mut planes = Vec::new();
    for (name, value) in &outputs {
        let (_shape, data) = value.try_extract_tensor::<f32>().map_err(|err| err.to_string())?;
        planes.push((name.to_string(), data.to_vec()));
    }
    Ok(planes)
}

/// YuNet names each output by kind and stride. Those names are the layout.
fn yunet_planes(outputs: &[(String, Vec<f32>)]) -> Option<yunet::Planes<'_>> {
    let find = |kind: &str, stride: u32| {
        let name = format!("{kind}_{stride}");
        outputs.iter().find(|(n, _)| *n == name).map(|(_, data)| data.as_slice())
    };
    let level = |kind: &str| -> Option<[&[f32]; 3]> { Some([find(kind, 8)?, find(kind, 16)?, find(kind, 32)?]) };
    Some(yunet::Planes { cls: level("cls")?, obj: level("obj")?, bbox: level("bbox")?, kps: level("kps")? })
}

impl Perception for FaceModels {
    fn kind(&self) -> &'static str {
        PERCEPTION_ONNX
    }

    fn provider(&self) -> &str {
        self.provider
    }

    fn min_face_px(&self) -> u32 {
        match self.detector_kind {
            DetectorKind::Yunet => 28,
            DetectorKind::Scrfd => 24,
        }
    }

    fn high_score(&self) -> f32 {
        match self.detector_kind {
            DetectorKind::Yunet => 0.8,
            DetectorKind::Scrfd => 0.6,
        }
    }

    fn compare_score(&self) -> f32 {
        match self.detector_kind {
            DetectorKind::Yunet => 0.75,
            DetectorKind::Scrfd => 0.5,
        }
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn detect(&mut self, frame: &RgbImage, det_long: u32) -> Result<Vec<Detection>, String> {
        let (det_img, map) = geom::resize_long_side_smooth(frame, det_long);
        let faces = self.detect_on(&det_img)?;
        Ok(faces
            .into_iter()
            .filter_map(|face| {
                let rect = map.to_original(face.rect).clamp(frame.width(), frame.height())?;
                Some(Detection {
                    rect,
                    det_short: face.rect.short(),
                    score: face.score,
                    landmarks: face.landmarks.map(|points| map.points_to_original(points)),
                })
            })
            .collect())
    }

    /// Landmarks found on a face the network saw under 112 px land several
    /// original pixels off; on 12-megapixel photos of LFW faces that cost
    /// about 0.02 of cosine. Those landmarks are read again near the face.
    fn align(&mut self, frame: &RgbImage, det: &Detection, _index: u64) -> Result<Option<Aligned>, String> {
        let Some(points) = det.landmarks else {
            return Ok(None);
        };
        let seen = self.network_short(det, frame);
        let points = if seen < det.rect.short() as f32 && seen < CROP as f32 {
            self.landmarks_near(frame, det.rect)?.unwrap_or(points)
        } else {
            points
        };
        Ok(arcface::warp_112(frame, &points).map(Aligned::Face))
    }

    fn embed(&mut self, aligned: &Aligned) -> Result<Vec<f32>, String> {
        let Aligned::Face(face) = aligned else {
            return Err("The embedder was given a demo marker. Refusing.".into());
        };
        let layout = match self.embedder_kind {
            EmbedderKind::ArcFace => Layout::RgbArcFace,
            EmbedderKind::SFace => Layout::RgbRaw,
        };
        let outputs = run(&mut self.embedder, &to_tensor(face, layout))?;
        let (_name, mut embedding) = outputs.into_iter().next().ok_or("The embedder returned nothing. Refusing.")?;
        crate::perception::normalize(&mut embedding);
        Ok(embedding)
    }

    fn time(&mut self, det_long: u32) -> Option<Timing> {
        let frame = RgbImage::from_pixel(det_long, (det_long * 9 / 16).max(1), image::Rgb([96, 96, 96]));
        let face = Aligned::Face(RgbImage::from_pixel(CROP, CROP, image::Rgb([128, 128, 128])));
        // The first pass warms the runtime. The median of three follows.
        self.detect_on(&frame).ok()?;
        self.embed(&face).ok()?;
        let mut detect = Vec::new();
        let mut embed = Vec::new();
        for _ in 0..3 {
            let start = Instant::now();
            self.detect_on(&frame).ok()?;
            detect.push(start.elapsed().as_secs_f64() * 1000.0);
            let start = Instant::now();
            self.embed(&face).ok()?;
            embed.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        detect.sort_by(f64::total_cmp);
        embed.sort_by(f64::total_cmp);
        Some(Timing { detect_ms: detect[1], embed_ms: embed[1] })
    }
}

impl FaceModels {
    /// Faces on the detection image, in its pixels.
    fn detect_on(&mut self, image: &RgbImage) -> Result<Vec<FaceBox>, String> {
        let (w, h) = image.dimensions();
        if w == 0 || h == 0 {
            return Ok(Vec::new());
        }
        let whole = Rect { x: 0, y: 0, w, h };
        let Some(side) = self.side else {
            return self.faces_dynamic(image);
        };
        let fit = Letterbox::within(w, h, side);
        let square = letterbox_image(image, &fit, side);
        let mut faces = self.faces_in(&square, &fit, side, whole, (w, h))?;
        if fit.scale < 1.0 {
            for region in tiles(w, h, side) {
                let view = imageops::crop_imm(image, region.x, region.y, region.w, region.h).to_image();
                let tile_fit = Letterbox::within(region.w, region.h, side);
                let tile = letterbox_image(&view, &tile_fit, side);
                faces.extend(self.faces_in(&tile, &tile_fit, side, region, (w, h))?);
            }
            nms(&mut faces, 0.4);
        }
        Ok(faces)
    }

    /// A dynamic input reads the picture once, padded to the largest stride.
    fn faces_dynamic(&mut self, image: &RgbImage) -> Result<Vec<FaceBox>, String> {
        let (w, h) = image.dimensions();
        let (pw, ph) = (w.div_ceil(32) * 32, h.div_ceil(32) * 32);
        let mut padded = RgbImage::new(pw, ph);
        imageops::replace(&mut padded, image, 0, 0);
        let input = to_tensor(&padded, self.layout());
        let outputs = run(&mut self.detector, &input)?;
        let mut faces = self.decode(&outputs, pw, ph)?;
        faces.retain_mut(|face| match source_rect(face.rect.x as f32, face.rect.y as f32, (face.rect.x + face.rect.w) as f32, (face.rect.y + face.rect.h) as f32, (w, h)) {
            Some(rect) => {
                face.rect = rect;
                true
            }
            None => false,
        });
        Ok(faces)
    }

    fn layout(&self) -> Layout {
        match self.detector_kind {
            DetectorKind::Scrfd => Layout::RgbScrfd,
            DetectorKind::Yunet => Layout::BgrRaw,
        }
    }

    fn decode(&self, outputs: &[(String, Vec<f32>)], w: u32, h: u32) -> Result<Vec<FaceBox>, String> {
        match self.detector_kind {
            DetectorKind::Scrfd => scrfd_faces(outputs, w, h),
            DetectorKind::Yunet => {
                let planes = yunet_planes(outputs).ok_or("The detector output was not recognized. Refusing.")?;
                Ok(yunet::decode(w, h, &planes, YUNET_SCORE))
            }
        }
    }

    /// The short side of this face as the network saw it.
    fn network_short(&self, det: &Detection, frame: &RgbImage) -> f32 {
        let long = frame.width().max(frame.height()).max(1) as f32;
        let det_scale = det.det_short as f32 / det.rect.short().max(1) as f32;
        let det_long = long * det_scale;
        let input = self.side.map(|side| (side as f32 / det_long).min(1.0)).unwrap_or(1.0);
        det.det_short as f32 * input
    }

    /// Five landmarks from a second pass over the face's own neighbourhood of
    /// `frame`, twice the face across, where the network sees the face at up to
    /// half its input instead of shrunk with the whole frame.
    pub fn landmarks_near(&mut self, frame: &RgbImage, face: Rect) -> Result<Option<[(f32, f32); 5]>, String> {
        let (w, h) = frame.dimensions();
        let reach = face.w.max(face.h);
        let (cx, cy) = (face.x + face.w / 2, face.y + face.h / 2);
        let (x0, y0) = (cx.saturating_sub(reach), cy.saturating_sub(reach));
        let (x1, y1) = ((cx + reach).min(w), (cy + reach).min(h));
        if x1 <= x0 || y1 <= y0 {
            return Ok(None);
        }
        let region = Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
        let view = imageops::crop_imm(frame, region.x, region.y, region.w, region.h).to_image();
        let side = self.side.unwrap_or(640);
        let (view, map) = geom::resize_long_side_smooth(&view, side);
        let faces = self.detect_on(&view)?;
        Ok(faces
            .into_iter()
            .filter_map(|found| {
                let rect = map.to_original(found.rect);
                let rect = Rect { x: rect.x + region.x, y: rect.y + region.y, ..rect };
                let points = found.landmarks?.map(|(x, y)| map.point_to_original((x, y)));
                let points = points.map(|(x, y)| (x + region.x as f32, y + region.y as f32));
                (rect.iou(face) >= 0.5).then_some((rect.iou(face), points))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, points)| points))
    }

    /// Faces on one network input, in the detection image's pixels.
    /// A tile box that touches an inside edge of that tile is left to the
    /// neighbouring tile or the whole-image pass, where it is not cut.
    fn faces_in(&mut self, square: &RgbImage, fit: &Letterbox, side: u32, region: Rect, size: (u32, u32)) -> Result<Vec<FaceBox>, String> {
        let input = to_tensor(square, self.layout());
        let outputs = run(&mut self.detector, &input)?;
        let decoded = self.decode(&outputs, side, side)?;
        let tiled = region.w != size.0 || region.h != size.1;
        let mut out = Vec::new();
        for face in decoded {
            let map = |x: f32, y: f32| ((x - fit.pad_x.round()) / fit.scale + region.x as f32, (y - fit.pad_y.round()) / fit.scale + region.y as f32);
            let (x1, y1) = map(face.rect.x as f32, face.rect.y as f32);
            let (x2, y2) = map((face.rect.x + face.rect.w) as f32, (face.rect.y + face.rect.h) as f32);
            if tiled && touches_inner_edge((x1, y1, x2, y2), region, size) {
                continue;
            }
            let Some(rect) = source_rect(x1, y1, x2, y2, size) else {
                continue;
            };
            let landmarks = face.landmarks.map(|points| points.map(|(x, y)| map(x, y)));
            out.push(FaceBox { rect, score: face.score, landmarks });
        }
        Ok(out)
    }
}

fn scrfd_faces(planes: &[(String, Vec<f32>)], w: u32, h: u32) -> Result<Vec<FaceBox>, String> {
    if planes.len() != 6 && planes.len() != 9 {
        return Err("The detector output was not recognized. Refusing.".into());
    }
    let p = |i: usize| planes[i].1.as_slice();
    let kps = if planes.len() == 9 { [Some(p(6)), Some(p(7)), Some(p(8))] } else { [None, None, None] };
    Ok(scrfd::decode(w, h, [p(0), p(1), p(2)], [p(3), p(4), p(5)], kps, SCRFD_SCORE))
}

fn touches_inner_edge((x1, y1, x2, y2): (f32, f32, f32, f32), region: Rect, (w, h): (u32, u32)) -> bool {
    let margin = 2.0;
    (region.x > 0 && x1 <= region.x as f32 + margin)
        || (region.y > 0 && y1 <= region.y as f32 + margin)
        || (region.x + region.w < w && x2 >= (region.x + region.w) as f32 - margin)
        || (region.y + region.h < h && y2 >= (region.y + region.h) as f32 - margin)
}

fn source_rect(x1: f32, y1: f32, x2: f32, y2: f32, (w, h): (u32, u32)) -> Option<Rect> {
    let left = x1.clamp(0.0, w as f32);
    let top = y1.clamp(0.0, h as f32);
    let right = x2.clamp(0.0, w as f32);
    let bottom = y2.clamp(0.0, h as f32);
    let rw = (right - left).round();
    let rh = (bottom - top).round();
    if !(rw >= 1.0 && rh >= 1.0) {
        return None;
    }
    Some(Rect { x: left.round() as u32, y: top.round() as u32, w: rw as u32, h: rh as u32 })
}

/// Overlapping squares that cover a picture larger than the network input.
fn tiles(w: u32, h: u32, side: u32) -> Vec<Rect> {
    let starts = |length: u32| -> Vec<u32> {
        if length <= side {
            return vec![0];
        }
        let step = side.saturating_sub(TILE_OVERLAP).max(1);
        let mut out: Vec<u32> = (0..).map(|i| i * step).take_while(|start| start + side < length).collect();
        out.push(length - side);
        out
    };
    let mut out = Vec::new();
    for y in starts(h) {
        for x in starts(w) {
            out.push(Rect { x, y, w: side.min(w), h: side.min(h) });
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Layout {
    RgbScrfd,
    RgbArcFace,
    RgbRaw,
    BgrRaw,
}

/// NCHW float input. One pass over the pixels, writing each plane in order.
fn to_tensor(image: &RgbImage, layout: Layout) -> Array4<f32> {
    let (w, h) = image.dimensions();
    let plane = (w * h) as usize;
    let mut data = vec![0f32; 3 * plane];
    let (scale, offset, bgr) = match layout {
        Layout::RgbScrfd => (1.0 / 128.0, -127.5 / 128.0, false),
        Layout::RgbArcFace => (1.0 / 127.5, -1.0, false),
        Layout::RgbRaw => (1.0, 0.0, false),
        Layout::BgrRaw => (1.0, 0.0, true),
    };
    for (i, pixel) in image.pixels().enumerate() {
        for c in 0..3 {
            let channel = if bgr { 2 - c } else { c };
            data[channel * plane + i] = f32::from(pixel[c]) * scale + offset;
        }
    }
    Array4::from_shape_vec((1, 3, h as usize, w as usize), data).expect("tensor shape matches its data")
}

/// The picture on a black square of side `dst`, shrunk by `fit.scale` with an
/// area filter and centered on `fit`'s pad. Every network pixel inside the
/// picture is a picture pixel.
fn letterbox_image(image: &RgbImage, fit: &Letterbox, dst: u32) -> RgbImage {
    let mut out = RgbImage::from_pixel(dst, dst, image::Rgb([0, 0, 0]));
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return out;
    }
    let cw = ((w as f32 * fit.scale).round() as u32).clamp(1, dst);
    let ch = ((h as f32 * fit.scale).round() as u32).clamp(1, dst);
    let content = if (cw, ch) == (w, h) { image.clone() } else { imageops::resize(image, cw, ch, FilterType::Triangle) };
    let ox = (fit.pad_x.round().max(0.0) as u32).min(dst - cw);
    let oy = (fit.pad_y.round().max(0.0) as u32).min(dst - ch);
    imageops::replace(&mut out, &content, i64::from(ox), i64::from(oy));
    out
}

impl FrameMap {
    /// A detection-image point on the original frame. Landmarks are continuous
    /// coordinates from the image corner, so they scale with the image.
    pub fn point_to_original(self, (x, y): (f32, f32)) -> (f32, f32) {
        (x * self.det_to_orig_x, y * self.det_to_orig_y)
    }

    pub fn points_to_original(self, points: [(f32, f32); 5]) -> [(f32, f32); 5] {
        points.map(|p| self.point_to_original(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn a_small_detection_image_reaches_the_network_without_holes() {
        // A 320 px detection image used to be spread over the 640 px input one
        // source pixel at a time, so every other row and column stayed black.
        let image = RgbImage::from_pixel(320, 240, Rgb([200, 180, 160]));
        let fit = Letterbox::fit(320, 240, 640);
        let square = letterbox_image(&image, &fit, 640);
        let (x0, y0) = (fit.pad_x.ceil() as u32, fit.pad_y.ceil() as u32);
        let (x1, y1) = ((640.0 - fit.pad_x).floor() as u32, (640.0 - fit.pad_y).floor() as u32);
        let holes = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| square.get_pixel(x, y).0 == [0, 0, 0])
            .count();
        assert_eq!(holes, 0);
    }

    #[test]
    fn tensors_are_planar_in_the_model_channel_order() {
        let image = RgbImage::from_pixel(2, 1, Rgb([10, 20, 30]));
        let bgr = to_tensor(&image, Layout::BgrRaw);
        assert_eq!(bgr[[0, 0, 0, 1]], 30.0);
        assert_eq!(bgr[[0, 2, 0, 0]], 10.0);
        let arc = to_tensor(&image, Layout::RgbArcFace);
        assert!((arc[[0, 0, 0, 0]] - (10.0 - 127.5) / 127.5).abs() < 1e-6);
    }

    #[test]
    fn a_point_scales_with_the_image() {
        let map = FrameMap::uniform(4.0);
        assert_eq!(map.point_to_original((2.5, 1.0)), (10.0, 4.0));
        assert_eq!(FrameMap::identity().point_to_original((3.0, 7.0)), (3.0, 7.0));
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use crate::bundle::{load_bundle, tests::dev_bundle};
    use std::path::PathBuf;

    #[test]
    fn pinned_stub_models_load_from_their_verified_bytes() {
        let models = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/models");
        let root = tempfile::tempdir().unwrap();
        let det = std::fs::read(models.join("detector.onnx")).unwrap();
        let emb = std::fs::read(models.join("embedder.onnx")).unwrap();
        let dir = dev_bundle(root.path(), "dev", 0.5, &[("detector", "det.onnx", &det), ("embedder", "emb.onnx", &emb)]);
        let bundle = load_bundle(&dir).unwrap();
        assert!(crate::runtime::linked(), "candidates: {:?}", crate::runtime::library_candidates());
        let result = load(&bundle, false);
        match result {
            Ok(mut face) => {
                assert_eq!(face.provider(), "CPUExecutionProvider");
                assert!(face.detect(&RgbImage::from_pixel(48, 36, image::Rgb([12, 24, 36])), 640).unwrap().is_empty());
                assert!(face.dim() >= 128);
            }
            Err(message) => panic!("{message}"),
        }
    }
}
