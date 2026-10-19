// SPDX-License-Identifier: Apache-2.0

//! Measurement helper for `eval/real_faces.py`. It is not shipped.
//!
//! `vectors` prints, for each picture, every face the scan would see: its box,
//! what the quality gates said, and the vector it would compare. It goes
//! through the library's own decode, detection size, gates, alignment, and
//! embedder (`perception::face_vectors`), on the CPU.
//!
//! `pack` turns a list of photos into a network poster pack bound to the
//! bundle's embedder. The shipping `openworld` command has no way to do that,
//! so a person cannot enroll a stranger's photo with it.

use image::RgbImage;
use openworld_core::bundle::{load_bundles, Bundle};
use openworld_core::decode::for_each_frame;
use openworld_core::onnx_exec::{self, FaceModels};
use openworld_core::perception::{auto_long_side, face_vectors, FaceVector, Perception};
use openworld_core::posters::{write_network_pack, Poster, PosterClass};
use serde_json::json;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("vectors") => vectors(&args[1..]),
        Some("pack") => pack(&args[1..]),
        _ => Err("usage: openworld-eval vectors|pack --bundles DIR --bundle ID --list FILE [--enlarge F] ...".into()),
    };
    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(2);
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn bundle(args: &[String]) -> Result<Bundle, String> {
    let dir = flag(args, "--bundles").ok_or("--bundles is required")?;
    let id = flag(args, "--bundle").ok_or("--bundle is required")?;
    let all = load_bundles(Path::new(dir)).map_err(|line| line.text)?;
    all.into_iter().find(|b| b.id == id).ok_or_else(|| format!("{id} is not in the catalog"))
}

/// The bundle's networks on the CPU. The curve is measured there; `openworld bench`
/// reports how far an accelerated provider moves the scores.
fn models(bundle: &Bundle) -> Result<FaceModels, String> {
    onnx_exec::load(bundle, false)
}

fn enlarge_factor(args: &[String]) -> Result<f32, String> {
    let factor: f32 = flag(args, "--enlarge").unwrap_or("1").parse().map_err(|_| "bad --enlarge")?;
    if !(factor > 0.0 && factor <= 8.0) {
        return Err("--enlarge must be above 0 and at most 8".into());
    }
    Ok(factor)
}

/// The first picture of a file, the way the scan decodes it (turned upright).
fn first_frame(path: &Path) -> Option<RgbImage> {
    let mut first = None;
    for_each_frame(path, |_, frame| {
        first = Some(frame.clone());
        false
    })
    .ok()?;
    first
}

/// LFW faces are about 94 px. Enlarging the picture lets a face pass the
/// 112 px compare rule; the embedder still reads the same 112 px aligned crop.
fn enlarged(frame: RgbImage, factor: f32) -> RgbImage {
    if factor == 1.0 {
        return frame;
    }
    let w = (frame.width() as f32 * factor).round().max(1.0) as u32;
    let h = (frame.height() as f32 * factor).round().max(1.0) as u32;
    image::imageops::resize(&frame, w, h, image::imageops::FilterType::CatmullRom)
}

/// `reject` is the quality-gate code (`why_small`, `why_pose`, ...) or null when compared.
fn face_json(face: &FaceVector) -> serde_json::Value {
    json!({
        "rect": [face.rect.x, face.rect.y, face.rect.w, face.rect.h],
        "det_short": face.det_short,
        "score": face.score,
        "reject": face.reject,
        "embedding": face.embedding,
    })
}

fn vectors(args: &[String]) -> Result<(), String> {
    let bundle = bundle(args)?;
    let factor = enlarge_factor(args)?;
    let list = flag(args, "--list").ok_or("--list is required")?;
    let mut models = models(&bundle)?;
    let reader = std::io::BufReader::new(std::fs::File::open(list).map_err(|e| e.to_string())?);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in reader.lines() {
        let path = PathBuf::from(line.map_err(|e| e.to_string())?.trim());
        if path.as_os_str().is_empty() {
            continue;
        }
        let row = match first_frame(&path) {
            None => json!({"path": path, "error": "decode"}),
            Some(frame) => {
                let frame = enlarged(frame, factor);
                let det_long = auto_long_side(frame.width().max(frame.height()), models.min_face_px());
                let faces = face_vectors(&mut models, &frame)?;
                json!({
                    "path": path,
                    "width": frame.width(),
                    "height": frame.height(),
                    "detection_long_side": det_long,
                    "provider": models.provider(),
                    "faces": faces.iter().map(face_json).collect::<Vec<_>>(),
                })
            }
        };
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The detected face nearest the middle of the picture. A poster photo, like
/// an LFW photo, centers its subject; a face beside it is someone else.
fn center_face(faces: Vec<FaceVector>, frame: &RgbImage) -> Option<FaceVector> {
    let (cx, cy) = (frame.width() as f64 / 2.0, frame.height() as f64 / 2.0);
    let distance = |f: &FaceVector| {
        let x = f.rect.x as f64 + f.rect.w as f64 / 2.0 - cx;
        let y = f.rect.y as f64 + f.rect.h as f64 / 2.0 - cy;
        x * x + y * y
    };
    faces.into_iter().min_by(|a, b| distance(a).total_cmp(&distance(b)))
}

/// Each line is `id <TAB> missing|wanted <TAB> title <TAB> photo`. The center
/// face of the photo is that poster's vector. A photo whose center face was
/// not compared is left out, and named in the output.
fn pack(args: &[String]) -> Result<(), String> {
    let bundle = bundle(args)?;
    let factor = enlarge_factor(args)?;
    let list = flag(args, "--list").ok_or("--list is required")?;
    let out = PathBuf::from(flag(args, "--out").ok_or("--out is required")?);
    let source = flag(args, "--source").unwrap_or("evaluation");
    let embedder = bundle
        .weights
        .iter()
        .find(|w| w.role == "embedder" && w.present)
        .map(|w| w.sha256.clone())
        .ok_or("the embedder weight is not installed")?;
    let mut models = models(&bundle)?;
    let mut posters = Vec::new();
    let mut left_out = Vec::new();
    let text = std::fs::read_to_string(list).map_err(|e| e.to_string())?;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let cols: Vec<&str> = line.split('\t').collect();
        let [id, class, title, photo] = cols[..] else {
            return Err(format!("bad line: {line}"));
        };
        let class = PosterClass::parse(class).ok_or_else(|| format!("bad class: {class}"))?;
        let Some(frame) = first_frame(Path::new(photo)) else {
            left_out.push(json!({"id": id, "reason": "decode"}));
            continue;
        };
        let frame = enlarged(frame, factor);
        let faces = face_vectors(&mut models, &frame)?;
        let face = center_face(faces, &frame);
        let Some(embedding) = face.as_ref().and_then(|f| f.embedding.clone()) else {
            let reason = face.and_then(|f| f.reject).unwrap_or_else(|| "no_face".into());
            left_out.push(json!({"id": id, "reason": reason}));
            continue;
        };
        posters.push(Poster {
            id: id.into(),
            class,
            title: title.into(),
            // An evaluation poster has no FBI page, so its card has no link.
            fbi_url: None,
            embedding: Some(embedding),
            image: None,
            image_sha256: None,
            expires_at: None,
        });
    }
    let now = SystemTime::now();
    let pack = write_network_pack(&out, "evaluation", source, &embedder, posters, now, now + Duration::from_secs(30 * 86400))
        .map_err(|line| line.text)?;
    println!("{}", json!({"posters": pack.posters.len(), "left_out": left_out, "out": out}));
    Ok(())
}
