// SPDX-License-Identifier: Apache-2.0

//! ONNX Runtime sessions for a bundle whose weights are pinned and present.
//!
//! InsightFace research-only files never reach this loader: the catalog refuses
//! them. A session that does not load, or whose outputs are not SCRFD, YuNet,
//! ArcFace, or SFace, refuses the scan. An empty detection list is not produced
//! in that case.
//!
//! The detector input is the square the model was exported with. A detection
//! image that fits is placed at its own size, so the 64 px rule is measured on
//! the pixels the network sees. A larger one is read twice: shrunk whole, for
//! large faces, and in overlapping tiles at its own size, for small ones.

use crate::arcface::{self, CROP};
use crate::bundle::Bundle;
use crate::fiducial::{Hit, Marker};
use crate::geom::Rect;
use crate::hardware::Execution;
use crate::scrfd::{self, nms, FaceBox, Letterbox};
use crate::yunet;
use image::imageops::{self, FilterType};
use image::RgbImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;
use std::sync::OnceLock;

/// SCRFD keeps a box at this score. Its outputs are already calibrated low.
const SCRFD_SCORE: f32 = 0.3;
/// YuNet keeps a box at this score. OpenCV's sample uses 0.9; the scan wants
/// fewer misses, and a weak box is only a crop under the cutoff.
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
    side: u32,
    embedder: Session,
    embedder_kind: EmbedderKind,
    plate: Option<Session>,
    pub execution: Execution,
}

pub fn active_execution() -> Execution {
    // The runtime downloaded for this build registers the CPU provider.
    let _ = ensure_runtime();
    Execution::Cpu
}

pub fn runtime_linked() -> bool {
    ensure_runtime().is_ok()
}

fn ensure_runtime() -> Result<(), String> {
    static READY: OnceLock<Result<(), String>> = OnceLock::new();
    READY
        .get_or_init(|| {
            if ort::init().commit() {
                Ok(())
            } else {
                Err("ONNX Runtime did not load. Refusing.".into())
            }
        })
        .clone()
}

pub fn load(bundle: &Bundle) -> Result<FaceModels, String> {
    ensure_runtime()?;
    let detector_path = weight_path(bundle, "detector")?;
    let embedder_path = weight_path(bundle, "embedder")?;
    let mut detector = Session::builder()
        .map_err(|err| err.to_string())?
        .commit_from_file(&detector_path)
        .map_err(|err| format!("The detector did not load. Refusing. {err}"))?;
    let mut embedder = Session::builder()
        .map_err(|err| err.to_string())?
        .commit_from_file(&embedder_path)
        .map_err(|err| format!("The embedder did not load. Refusing. {err}"))?;
    let side = square_side(&detector).unwrap_or(640);
    let detector_kind = check_detector(&mut detector, side)?;
    let embedder_kind = if bundle.embedder.to_ascii_lowercase().starts_with("sface") {
        EmbedderKind::SFace
    } else {
        EmbedderKind::ArcFace
    };
    check_embedder(&mut embedder)?;
    let plate = match weight_path(bundle, "plate") {
        Ok(path) => {
            let mut session = Session::builder()
                .map_err(|err| err.to_string())?
                .commit_from_file(&path)
                .map_err(|err| format!("The plate model did not load. Refusing. {err}"))?;
            check_plate(&mut session, side)?;
            Some(session)
        }
        Err(_) => None,
    };
    Ok(FaceModels {
        detector,
        detector_kind,
        side,
        embedder,
        embedder_kind,
        plate,
        execution: Execution::Cpu,
    })
}

fn weight_path(bundle: &Bundle, role: &str) -> Result<std::path::PathBuf, String> {
    let pin = bundle
        .weights
        .iter()
        .find(|weight| weight.role == role && weight.present)
        .ok_or_else(|| format!("The {role} weight is not pinned. Refusing."))?;
    Ok(bundle.dir.join(&pin.path))
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
    let outputs = run_zeros(session, side)?;
    match outputs.len() {
        6 | 9 => Ok(DetectorKind::Scrfd),
        12 if yunet_planes(&outputs).is_some() => Ok(DetectorKind::Yunet),
        _ => Err("The detector output was not recognized. Refusing.".into()),
    }
}

fn check_embedder(session: &mut Session) -> Result<(), String> {
    let outputs = run_zeros(session, CROP)?;
    let Some((_, first)) = outputs.first() else {
        return Err("The embedder output was not recognized. Refusing.".into());
    };
    if first.len() < 128 {
        return Err("The embedder output was not recognized. Refusing.".into());
    }
    Ok(())
}

fn check_plate(session: &mut Session, side: u32) -> Result<(), String> {
    let outputs = run_zeros(session, side)?;
    if outputs.is_empty() {
        return Err("The plate model output was not recognized. Refusing.".into());
    }
    Ok(())
}

fn run_zeros(session: &mut Session, side: u32) -> Result<Vec<(String, Vec<f32>)>, String> {
    let side = side as usize;
    let input = Array4::<f32>::zeros((1, 3, side, side));
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

impl FaceModels {
    pub fn detect(&mut self, image: &RgbImage) -> Result<Vec<Hit>, String> {
        let (w, h) = image.dimensions();
        if w == 0 || h == 0 {
            return Ok(Vec::new());
        }
        let whole = Rect { x: 0, y: 0, w, h };
        let fit = Letterbox::within(w, h, self.side);
        let square = letterbox_image(image, &fit, self.side);
        let mut faces = self.faces_in(&square, &fit, whole, (w, h))?;
        if fit.scale < 1.0 {
            for region in tiles(w, h, self.side) {
                let view = imageops::crop_imm(image, region.x, region.y, region.w, region.h).to_image();
                let tile_fit = Letterbox::within(region.w, region.h, self.side);
                let tile = letterbox_image(&view, &tile_fit, self.side);
                faces.extend(self.faces_in(&tile, &tile_fit, region, (w, h))?);
            }
            nms(&mut faces, 0.4);
        }
        let mut hits: Vec<Hit> = faces
            .into_iter()
            .map(|face| Hit { rect: face.rect, marker: Marker::Face { id: 0 }, score: face.score, landmarks: face.landmarks })
            .collect();
        if let Some(plate) = &mut self.plate {
            let input = to_tensor(&square, Layout::RgbArcFace);
            hits.extend(plates_from_session(plate, &input, &fit, (w, h))?);
        }
        Ok(hits)
    }

    /// Faces on one network input, in the detection image's pixels.
    /// A tile box that touches an inside edge of that tile is left to the
    /// neighbouring tile or the whole-image pass, where it is not cut.
    fn faces_in(&mut self, square: &RgbImage, fit: &Letterbox, region: Rect, size: (u32, u32)) -> Result<Vec<FaceBox>, String> {
        let layout = match self.detector_kind {
            DetectorKind::Scrfd => Layout::RgbScrfd,
            DetectorKind::Yunet => Layout::BgrRaw,
        };
        let input = to_tensor(square, layout);
        let outputs = run(&mut self.detector, &input)?;
        let decoded = match self.detector_kind {
            DetectorKind::Scrfd => scrfd_faces(&outputs, self.side)?,
            DetectorKind::Yunet => {
                let planes = yunet_planes(&outputs).ok_or("The detector output was not recognized. Refusing.")?;
                yunet::decode(self.side, self.side, &planes, YUNET_SCORE)
            }
        };
        let tiled = region.w != size.0 || region.h != size.1;
        let mut out = Vec::new();
        for face in decoded {
            let map = |x: f32, y: f32| ((x - fit.pad_x) / fit.scale + region.x as f32, (y - fit.pad_y) / fit.scale + region.y as f32);
            let (x1, y1) = map(face.rect.x as f32, face.rect.y as f32);
            let (x2, y2) = map((face.rect.x + face.rect.w) as f32, (face.rect.y + face.rect.h) as f32);
            if tiled && touches_inner_edge((x1, y1, x2, y2), region, size) {
                continue;
            }
            let Some(rect) = source_rect(x1, y1, x2, y2, size) else {
                continue;
            };
            let landmarks = face.landmarks.map(|points| {
                let mut mapped = [(0f32, 0f32); 5];
                for (slot, (x, y)) in mapped.iter_mut().zip(points) {
                    *slot = map(x, y);
                }
                mapped
            });
            out.push(FaceBox { rect, score: face.score, landmarks });
        }
        Ok(out)
    }

    /// `image` is the picture the landmarks are on. Pass the whole frame, so the
    /// aligned crop keeps the forehead and cheeks the face box leaves out.
    pub fn embed(&mut self, image: &RgbImage, landmarks: Option<[(f32, f32); 5]>) -> Result<Vec<f32>, String> {
        let aligned = if let Some(points) = landmarks {
            arcface::warp_112(image, &points).unwrap_or_else(|| resize_112(image))
        } else {
            resize_112(image)
        };
        let layout = match self.embedder_kind {
            EmbedderKind::ArcFace => Layout::RgbArcFace,
            EmbedderKind::SFace => Layout::RgbRaw,
        };
        let input = to_tensor(&aligned, layout);
        let outputs = run(&mut self.embedder, &input)?;
        let (_name, mut embedding) = outputs.into_iter().next().ok_or("The embedder returned nothing. Refusing.")?;
        arcface::l2_normalize(&mut embedding);
        Ok(embedding)
    }
}

fn scrfd_faces(planes: &[(String, Vec<f32>)], side: u32) -> Result<Vec<FaceBox>, String> {
    if planes.len() != 6 && planes.len() != 9 {
        return Err("The detector output was not recognized. Refusing.".into());
    }
    let p = |i: usize| planes[i].1.as_slice();
    let kps = if planes.len() == 9 { [Some(p(6)), Some(p(7)), Some(p(8))] } else { [None, None, None] };
    Ok(scrfd::decode(side, side, [p(0), p(1), p(2)], [p(3), p(4), p(5)], kps, SCRFD_SCORE))
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

fn to_tensor(image: &RgbImage, layout: Layout) -> Array4<f32> {
    let (w, h) = image.dimensions();
    let mut out = Array4::<f32>::zeros((1, 3, h as usize, w as usize));
    for (x, y, pixel) in image.enumerate_pixels() {
        for c in 0..3 {
            let (channel, value) = match layout {
                Layout::RgbScrfd => (c, (pixel[c] as f32 - 127.5) / 128.0),
                Layout::RgbArcFace => (c, (pixel[c] as f32 - 127.5) / 127.5),
                Layout::RgbRaw => (c, pixel[c] as f32),
                Layout::BgrRaw => (2 - c, pixel[c] as f32),
            };
            out[[0, channel, y as usize, x as usize]] = value;
        }
    }
    out
}

fn plates_from_session(
    session: &mut Session,
    input: &Array4<f32>,
    fit: &Letterbox,
    size: (u32, u32),
) -> Result<Vec<Hit>, String> {
    let outputs = session
        .run(ort::inputs![
            TensorRef::from_array_view(input).map_err(|err| err.to_string())?
        ])
        .map_err(|err| err.to_string())?;
    let mut planes = Vec::new();
    for (_name, value) in &outputs {
        if let Ok((_shape, data)) = value.try_extract_tensor::<f32>() {
            planes.push(data.to_vec());
        }
    }
    // End-to-end boxes are [N, 6] as x1,y1,x2,y2,score,class when a model exports them that way.
    // A detector that does not is still loaded; it contributes no plate text.
    let mut hits = Vec::new();
    if let Some(dets) = planes
        .iter()
        .find(|plane| !plane.is_empty() && plane.len() % 6 == 0 && plane.len() <= 6 * 200)
    {
        for chunk in dets.chunks(6) {
            let score = chunk[4];
            if score < 0.3 {
                continue;
            }
            let class = chunk[5].round() as i32;
            let map = |x: f32, y: f32| ((x - fit.pad_x) / fit.scale, (y - fit.pad_y) / fit.scale);
            let (x1, y1) = map(chunk[0], chunk[1]);
            let (x2, y2) = map(chunk[2], chunk[3]);
            let Some(rect) = source_rect(x1, y1, x2, y2, size) else {
                continue;
            };
            // COCO vehicles are not people. A plate class is whatever the bundle's plate model calls a plate (class 0 when it is a plate-only head).
            let marker = if class == 0 {
                Marker::Plate
            } else {
                Marker::Vehicle
            };
            hits.push(Hit {
                rect,
                marker,
                score,
                landmarks: None,
            });
        }
    }
    Ok(hits)
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

fn resize_112(image: &RgbImage) -> RgbImage {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return RgbImage::new(CROP, CROP);
    }
    imageops::resize(image, CROP, CROP, FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::load_bundle;
    use crate::posters::sha256_hex;
    use image::Rgb;
    use std::fs;
    use std::path::PathBuf;

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
    fn a_zero_session_detects_nothing_and_still_embeds_a_crop() {
        let dir = tempfile::tempdir().unwrap();
        let bundle_dir = dir.path().join("custom");
        fs::create_dir_all(bundle_dir.join("weights")).unwrap();
        let models = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/models");
        let mut manifest = r#"
schema = "openworld.bundle.v1"
id = "custom"
name = "Custom"
version = "0.0.0"
official = true
best_for = "A computer, when the weights are real."
threshold = 0.55
estimate_factor = 1.0

[models]
detector = "SCRFD-0.5GF"
detector_version = "pinned-test"
embedder = "ArcFace-MBF"
embedder_version = "pinned-test"
plate = "RTMDet-nano"
plate_version = "pinned-test"
plate_license = "Apache-2.0"
"#
        .to_string();
        for (file, role, model_name) in [
            ("detector.onnx", "detector", "SCRFD-0.5GF"),
            ("embedder.onnx", "embedder", "ArcFace-MBF"),
            ("plate.onnx", "plate", "RTMDet-nano"),
        ] {
            let bytes = fs::read(models.join(file)).unwrap();
            let sha = sha256_hex(&bytes);
            fs::write(bundle_dir.join("weights").join(file), &bytes).unwrap();
            manifest.push_str(&format!(
                "\n[[files]]\nrole = \"{role}\"\nname = \"{model_name}\"\nversion = \"pinned-test\"\nsha256 = \"{sha}\"\nlicense = \"Apache-2.0\"\npath = \"weights/{file}\"\n"
            ));
        }
        fs::write(bundle_dir.join("manifest.toml"), manifest).unwrap();
        let bundle = load_bundle(&bundle_dir).unwrap();
        assert!(bundle.weights_ready);
        assert!(runtime_linked());
        assert_eq!(active_execution(), Execution::Cpu);
        let mut face = load(&bundle).unwrap();
        let image = RgbImage::from_pixel(48, 36, Rgb([12, 24, 36]));
        assert!(face.detect(&image).unwrap().is_empty());
        let crop = RgbImage::from_pixel(40, 40, Rgb([200, 10, 10]));
        let plain = face.embed(&crop, None).unwrap();
        assert!(plain.len() >= 128);
        let points = [
            (8.0, 10.0),
            (30.0, 10.0),
            (18.0, 20.0),
            (10.0, 32.0),
            (28.0, 32.0),
        ];
        let aligned = face.embed(&crop, Some(points)).unwrap();
        assert_eq!(aligned.len(), plain.len());
        let empty = face.embed(&RgbImage::new(0, 0), None).unwrap();
        assert_eq!(empty.len(), plain.len());
    }
}
