// SPDX-License-Identifier: Apache-2.0

//! Measurement helper for `eval/real_faces.py`. It is not shipped.
//!
//! `vectors` prints, for each picture, the faces the scan would see and the
//! vectors it would compare, through the library's own decode and rules.
//! `pack` turns a list of photos into a network poster pack bound to the
//! bundle's embedder. The shipping `openworld` command has no way to do that,
//! so a person cannot enroll a stranger's photo with it.

use openworld_core::bundle::{load_bundles, Bundle};
use openworld_core::decode::for_each_frame;
use openworld_core::estimate::DetectionSize;
use openworld_core::onnx_exec;
use openworld_core::posters::{write_network_pack, Poster, PosterClass};
use openworld_core::scan::{face_vectors, FaceVector};
use image::RgbImage;
use serde_json::json;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("vectors") => vectors(&args[1..]),
        Some("pack") => pack(&args[1..]),
        _ => Err("usage: openworld-eval vectors|pack --bundles DIR --bundle ID ...".into()),
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
    let all = load_bundles(Path::new(dir)).map_err(|err| err.refusal())?;
    all.into_iter().find(|b| b.id == id).ok_or_else(|| format!("{id} is not in the catalog"))
}

fn detection(args: &[String]) -> Result<DetectionSize, String> {
    match flag(args, "--detection").unwrap_or("640") {
        "full" => Ok(DetectionSize::Full),
        n => n.parse().map(DetectionSize::Px).map_err(|_| format!("bad --detection {n}")),
    }
}

/// The first picture of a file, the way the scan decodes it.
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
    let w = (frame.width() as f32 * factor).round() as u32;
    let h = (frame.height() as f32 * factor).round() as u32;
    image::imageops::resize(&frame, w, h, image::imageops::FilterType::CatmullRom)
}

fn face_json(face: &FaceVector) -> serde_json::Value {
    json!({
        "rect": [face.rect.x, face.rect.y, face.rect.w, face.rect.h],
        "det_short_px": face.det_short_px,
        "score": face.score,
        "embedding": face.embedding,
    })
}

fn vectors(args: &[String]) -> Result<(), String> {
    let bundle = bundle(args)?;
    let detection = detection(args)?;
    let factor: f32 = flag(args, "--enlarge").unwrap_or("1").parse().map_err(|_| "bad --enlarge")?;
    let list = flag(args, "--list").ok_or("--list is required")?;
    let mut models = onnx_exec::load(&bundle)?;
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
                let faces = face_vectors(&mut models, &frame, detection)?;
                json!({
                    "path": path,
                    "width": frame.width(),
                    "height": frame.height(),
                    "faces": faces.iter().map(face_json).collect::<Vec<_>>(),
                })
            }
        };
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Each line is `id <TAB> missing|wanted <TAB> title <TAB> photo`. The largest
/// compared face on the photo is that poster's vector. A photo without one is
/// left out and named on stderr.
fn pack(args: &[String]) -> Result<(), String> {
    let bundle = bundle(args)?;
    let factor: f32 = flag(args, "--enlarge").unwrap_or("1").parse().map_err(|_| "bad --enlarge")?;
    let list = flag(args, "--list").ok_or("--list is required")?;
    let out = PathBuf::from(flag(args, "--out").ok_or("--out is required")?);
    let source = flag(args, "--source").unwrap_or("evaluation");
    let embedder = bundle
        .weights
        .iter()
        .find(|w| w.role == "embedder" && w.present)
        .map(|w| w.sha256.clone())
        .ok_or("the embedder weight is not installed")?;
    let mut models = onnx_exec::load(&bundle)?;
    let mut posters = Vec::new();
    let text = std::fs::read_to_string(list).map_err(|e| e.to_string())?;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let cols: Vec<&str> = line.split('\t').collect();
        let [id, class, title, photo] = cols[..] else {
            return Err(format!("bad line: {line}"));
        };
        let class = PosterClass::parse(class).ok_or_else(|| format!("bad class: {class}"))?;
        let Some(frame) = first_frame(Path::new(photo)) else {
            eprintln!("{id}: decode");
            continue;
        };
        let frame = enlarged(frame, factor);
        let faces = face_vectors(&mut models, &frame, DetectionSize::Px(640))?;
        let Some(best) = faces
            .into_iter()
            .filter(|f| f.embedding.is_some())
            .max_by_key(|f| f.rect.w as u64 * f.rect.h as u64)
        else {
            eprintln!("{id}: no compared face");
            continue;
        };
        posters.push(Poster {
            id: id.into(),
            class,
            title: title.into(),
            fbi_url: String::new(),
            embedding: best.embedding,
            fiducial_id: None,
            plate: None,
            expires_at: None,
        });
    }
    let now = SystemTime::now();
    let pack = write_network_pack(&out, "evaluation", source, &embedder, posters, now, now + Duration::from_secs(30 * 86400))
        .map_err(|err| err.refusal().to_string())?;
    println!("{}", json!({"posters": pack.posters.len(), "out": out}));
    Ok(())
}
