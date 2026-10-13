// SPDX-License-Identifier: Apache-2.0

//! ONNX Runtime sessions for a bundle whose weights are pinned and present.
//!
//! InsightFace research-only files never reach this loader: the catalog refuses
//! them. A session that does not load, or whose outputs are not SCRFD / ArcFace,
//! refuses the scan. An empty detection list is not produced in that case.

use crate::arcface::{self, CROP};
use crate::bundle::Bundle;
use crate::fiducial::{Hit, Marker};
use crate::geom::Rect;
use crate::hardware::Execution;
use crate::scrfd::{self, Letterbox};
use image::RgbImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;
use std::sync::OnceLock;

pub struct FaceModels {
    detector: Session,
    embedder: Session,
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
    check_detector(&mut detector)?;
    check_embedder(&mut embedder)?;
    let plate = match weight_path(bundle, "plate") {
        Ok(path) => {
            let mut session = Session::builder()
                .map_err(|err| err.to_string())?
                .commit_from_file(&path)
                .map_err(|err| format!("The plate model did not load. Refusing. {err}"))?;
            check_plate(&mut session)?;
            Some(session)
        }
        Err(_) => None,
    };
    Ok(FaceModels {
        detector,
        embedder,
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

fn check_detector(session: &mut Session) -> Result<(), String> {
    let outputs = run_zeros(session, 640)?;
    if outputs.len() != 6 && outputs.len() != 9 {
        return Err("The detector output was not recognized. Refusing.".into());
    }
    Ok(())
}

fn check_embedder(session: &mut Session) -> Result<(), String> {
    let outputs = run_zeros(session, CROP)?;
    let Some(first) = outputs.first() else {
        return Err("The embedder output was not recognized. Refusing.".into());
    };
    if first.len() < 128 {
        return Err("The embedder output was not recognized. Refusing.".into());
    }
    Ok(())
}

fn check_plate(session: &mut Session) -> Result<(), String> {
    let outputs = run_zeros(session, 640)?;
    if outputs.is_empty() {
        return Err("The plate model output was not recognized. Refusing.".into());
    }
    Ok(())
}

fn run_zeros(session: &mut Session, side: u32) -> Result<Vec<Vec<f32>>, String> {
    let side = side as usize;
    let input = Array4::<f32>::zeros((1, 3, side, side));
    let outputs = session
        .run(ort::inputs![
            TensorRef::from_array_view(&input).map_err(|err| err.to_string())?
        ])
        .map_err(|err| err.to_string())?;
    let mut planes = Vec::new();
    for (_name, value) in &outputs {
        let (_shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|err| err.to_string())?;
        planes.push(data.to_vec());
    }
    Ok(planes)
}

impl FaceModels {
    pub fn detect(&mut self, image: &RgbImage) -> Result<Vec<Hit>, String> {
        let (w, h) = image.dimensions();
        let fit = Letterbox::fit(w, h, 640);
        let square = letterbox_image(image, &fit, 640);
        let input = Array4::from_shape_vec((1, 3, 640, 640), arcface::nchw(&square))
            .map_err(|err| err.to_string())?;
        let outputs = self
            .detector
            .run(ort::inputs![
                TensorRef::from_array_view(&input).map_err(|err| err.to_string())?
            ])
            .map_err(|err| err.to_string())?;
        let mut planes = Vec::new();
        for (_name, value) in &outputs {
            let (_shape, data) = value
                .try_extract_tensor::<f32>()
                .map_err(|err| err.to_string())?;
            planes.push(data.to_vec());
        }
        let hits = faces_from_planes(&planes, &fit)?;
        let mut all = hits;
        if let Some(plate) = &mut self.plate {
            all.extend(plates_from_session(plate, &input)?);
        }
        Ok(all)
    }

    pub fn embed(
        &mut self,
        crop: &RgbImage,
        landmarks: Option<[(f32, f32); 5]>,
    ) -> Result<Vec<f32>, String> {
        let aligned = if let Some(points) = landmarks {
            arcface::warp_112(crop, &points).unwrap_or_else(|| resize_112(crop))
        } else {
            resize_112(crop)
        };
        let input = Array4::from_shape_vec(
            (1, 3, CROP as usize, CROP as usize),
            arcface::nchw(&aligned),
        )
        .map_err(|err| err.to_string())?;
        let outputs = self
            .embedder
            .run(ort::inputs![
                TensorRef::from_array_view(&input).map_err(|err| err.to_string())?
            ])
            .map_err(|err| err.to_string())?;
        let (_name, value) = (&outputs)
            .into_iter()
            .next()
            .ok_or("The embedder returned nothing. Refusing.")?;
        let (_shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|err| err.to_string())?;
        let mut embedding = data.to_vec();
        arcface::l2_normalize(&mut embedding);
        Ok(embedding)
    }
}

fn faces_from_planes(planes: &[Vec<f32>], fit: &Letterbox) -> Result<Vec<Hit>, String> {
    if planes.len() != 6 && planes.len() != 9 {
        return Err("The detector output was not recognized. Refusing.".into());
    }
    let kps = if planes.len() == 9 {
        [
            Some(planes[6].as_slice()),
            Some(planes[7].as_slice()),
            Some(planes[8].as_slice()),
        ]
    } else {
        [None, None, None]
    };
    let decoded = scrfd::decode(
        640,
        640,
        [
            planes[0].as_slice(),
            planes[1].as_slice(),
            planes[2].as_slice(),
        ],
        [
            planes[3].as_slice(),
            planes[4].as_slice(),
            planes[5].as_slice(),
        ],
        kps,
        0.3,
    );
    let mut hits = Vec::new();
    for face in decoded {
        let Some(rect) = fit.to_source(face.rect) else {
            continue;
        };
        let landmarks = face.landmarks.map(|points| {
            let mut mapped = [(0.0f32, 0.0f32); 5];
            for (i, (x, y)) in points.iter().enumerate() {
                mapped[i] = ((x - fit.pad_x) / fit.scale, (y - fit.pad_y) / fit.scale);
            }
            mapped
        });
        hits.push(Hit {
            rect,
            marker: Marker::Face { id: 0 },
            score: face.score,
            landmarks,
        });
    }
    Ok(hits)
}

fn plates_from_session(session: &mut Session, input: &Array4<f32>) -> Result<Vec<Hit>, String> {
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
            let rect = Rect {
                x: chunk[0].max(0.0) as u32,
                y: chunk[1].max(0.0) as u32,
                w: (chunk[2] - chunk[0]).max(1.0) as u32,
                h: (chunk[3] - chunk[1]).max(1.0) as u32,
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

fn letterbox_image(image: &RgbImage, fit: &Letterbox, dst: u32) -> RgbImage {
    let mut out = RgbImage::from_pixel(dst, dst, image::Rgb([0, 0, 0]));
    let (w, h) = image.dimensions();
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 * fit.scale + fit.pad_x).round() as i32;
            let dy = (y as f32 * fit.scale + fit.pad_y).round() as i32;
            if dx >= 0 && dy >= 0 && (dx as u32) < dst && (dy as u32) < dst {
                out.put_pixel(dx as u32, dy as u32, *image.get_pixel(x, y));
            }
        }
    }
    out
}

fn resize_112(image: &RgbImage) -> RgbImage {
    let mut out = RgbImage::new(CROP, CROP);
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return out;
    }
    for y in 0..CROP {
        for x in 0..CROP {
            let sx = (x as u32 * w / CROP).min(w - 1);
            let sy = (y as u32 * h / CROP).min(h - 1);
            out.put_pixel(x, y, *image.get_pixel(sx, sy));
        }
    }
    out
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
