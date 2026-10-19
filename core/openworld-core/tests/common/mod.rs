// SPDX-License-Identifier: Apache-2.0

//! Shared helpers for the integration tests: a demo poster pack in a temp
//! folder, scenes with demo markers placed where they pass (or miss) the
//! locked demo cutoff, FFmpeg encodes, file builders, and report lookups.
//!
//! Demo markers stand in for faces. A marker for poster 7 is the missing
//! poster and a marker for poster 11 the wanted one. A marker drawn with
//! 16-px modules is 128 px across, over the 112 px compare size; 8-px modules
//! give 64 px, which is seen and not compared.
//!
//! A demo score depends on where the marker sits and on the frame index, so
//! every scene here is placed where the marker passes on each frame it is on.
//! A track's template is the mean of some of those probes, so it passes too,
//! whichever frames the engine keeps. Neighbouring frames' scores are
//! correlated: no spot passes on eight frames in a row, so a marker stays on
//! at most six consecutive frames.

#![allow(dead_code)]

use image::{ImageEncoder, Rgb, RgbImage};
use openworld_core::bundle::load_bundles;
use openworld_core::copy::{self, Copy, Line};
use openworld_core::engine::{DecodeEnd, Engine, EngineConfig, Event, StopFlag};
use openworld_core::estimate::Coverage;
use openworld_core::fixture::{self, Markers, FIXTURE_CUTOFF};
use openworld_core::posters::{sha256_hex, write_fixture_pack};
use openworld_core::report::{Candidate, ScanReport, Status, TrackItem, TrackStatus};
use openworld_core::scan::{scan_path, ScanRequest};
use openworld_core::Bundle;
use serde_json::Value;
use std::cell::Cell;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

pub const MISSING_ID: u16 = 7;
pub const WANTED_ID: u16 = 11;
/// A marker id no poster carries.
pub const STRANGER_ID: u16 = 99;
pub const MISSING_POSTER: &str = "fixture-missing-a";
pub const WANTED_POSTER: &str = "fixture-wanted-b";
/// 16-px modules: 128 px across, compared.
pub const BIG: u32 = 16;
/// 8-px modules: 64 px across, seen and not compared.
pub const SMALL: u32 = 8;
pub const SIDE: u32 = 8 * BIG;
pub const W: u32 = 640;
pub const H: u32 = 480;

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn catalog() -> PathBuf {
    repo().join("bundles")
}

pub fn bundle(id: &str) -> Bundle {
    load_bundles(&catalog()).expect("catalog").into_iter().find(|b| b.id == id).expect("bundle")
}

pub fn days(n: u64) -> Duration {
    Duration::from_secs(n * 86_400)
}

// ---------------------------------------------------------------------------
// A temp folder with a demo poster pack.

pub struct Lab {
    pub dir: tempfile::TempDir,
    pub posters: PathBuf,
    outs: Cell<u32>,
}

impl Lab {
    pub fn new() -> Self {
        Self::dated(SystemTime::now())
    }

    /// A pack written as if fetched at `created`.
    pub fn dated(created: SystemTime) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let posters = dir.path().join("posters");
        write_fixture_pack(&posters, created).expect("demo pack");
        Self { dir, posters, outs: Cell::new(0) }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// A fresh result folder for each call.
    pub fn out(&self) -> PathBuf {
        let n = self.outs.get() + 1;
        self.outs.set(n);
        self.path(&format!("out-{n}"))
    }

    pub fn request(&self, input: impl AsRef<Path>) -> ScanRequest {
        ScanRequest::new(input.as_ref().to_path_buf(), catalog(), self.posters.clone(), self.out())
    }

    pub fn scan(&self, input: impl AsRef<Path>) -> ScanReport {
        run(&self.request(input))
    }

    pub fn scan_measured(&self, input: impl AsRef<Path>) -> ScanReport {
        let mut req = self.request(input);
        req.coverage = Coverage::Measured;
        run(&req)
    }

    /// Save a picture as PNG in the lab and return its path.
    pub fn png(&self, name: &str, image: &RgbImage) -> PathBuf {
        let path = self.path(name);
        image.save(&path).unwrap();
        path
    }

    pub fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    /// Edit the pack's snapshot and pin the edited bytes, as a re-signed pack would be.
    pub fn edit_pack(&self, edit: impl FnOnce(&mut Value)) {
        edit_pack(&self.posters, edit);
    }
}

pub fn run(req: &ScanRequest) -> ScanReport {
    scan_path(req, None, &mut |_| {})
}

pub fn edit_pack(dir: &Path, edit: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(&fs::read(dir.join("snapshot.json")).unwrap()).unwrap();
    edit(&mut value);
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(dir.join("snapshot.json"), &bytes).unwrap();
    fs::write(dir.join("snapshot.sha256"), format!("{}\n", sha256_hex(&bytes))).unwrap();
}

/// A developer bundle in `root/<id>`, labeled unofficial, with these weight files.
/// The same shape as the library's own test helper.
pub fn dev_bundle(root: &Path, id: &str, threshold: f32, files: &[(&str, &str, &[u8])]) -> PathBuf {
    let dir = root.join(id);
    fs::create_dir_all(dir.join("weights")).unwrap();
    let mut manifest = format!(
        "schema = \"openworld.bundle.v1\"\nid = \"{id}\"\nname = \"Dev\"\nversion = \"0\"\nofficial = false\nunofficial = true\nunofficial_notice = \"Unofficial developer bundle.\"\nbest_for = \"Tests.\"\nthreshold = {threshold}\nestimate_factor = 1.0\n\n[models]\ndetector = \"YuNet\"\ndetector_version = \"t\"\nembedder = \"SFace\"\nembedder_version = \"t\"\n"
    );
    for (role, file, bytes) in files {
        fs::write(dir.join("weights").join(file), bytes).unwrap();
        manifest.push_str(&format!(
            "\n[[files]]\nrole = \"{role}\"\nname = \"{file}\"\nversion = \"t\"\nsha256 = \"{}\"\nlicense = \"Apache-2.0\"\npath = \"weights/{file}\"\n",
            sha256_hex(bytes)
        ));
    }
    fs::write(dir.join("manifest.toml"), manifest).unwrap();
    dir
}

/// The constant-output ONNX graphs in `tests/models`. Not face networks.
pub fn stub_models() -> (Vec<u8>, Vec<u8>) {
    let models = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/models");
    (fs::read(models.join("detector.onnx")).unwrap(), fs::read(models.join("embedder.onnx")).unwrap())
}

// ---------------------------------------------------------------------------
// Scenes.

pub fn blank(w: u32, h: u32) -> RgbImage {
    openworld_core::geom::blank(w, h)
}

/// Blank frame `index`, with one dark pixel in the top row so an encoder keeps it
/// as its own picture. A single pixel is never read as a marker.
pub fn plain(index: u32) -> RgbImage {
    let mut image = blank(W, H);
    image.put_pixel(index % W, 0, Rgb([0, 0, 0]));
    image
}

pub fn place(canvas: &mut RgbImage, id: u16, module: u32, (x, y): (u32, u32)) {
    fixture::place(canvas, &fixture::render(id, module), x, y);
}

pub fn passes(id: u16, (x, y): (u32, u32), side: u32, frame: u64) -> bool {
    fixture::probe(id, x, y, side, side, frame).1 >= FIXTURE_CUTOFF
}

/// A corner inside `x0..x1` by `y0..y1` (for the corner) where a `side` px marker
/// for `id` passes on every frame in `frames`. Positions on the 16 px grid
/// come first, so JPEG and H.264 blocks line up with the marker's modules.
pub fn spot_in(id: u16, side: u32, frames: &[u64], xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> (u32, u32) {
    for step in [16usize, 2] {
        for y in ys.clone().step_by(step) {
            for x in xs.clone().step_by(step) {
                if frames.iter().all(|&f| passes(id, (x, y), side, f)) {
                    return (x, y);
                }
            }
        }
    }
    panic!("no spot for marker {id} passing on frames {frames:?}");
}

/// A spot for a 128 px marker anywhere in a 640x480 frame.
pub fn spot(id: u16, frames: &[u64]) -> (u32, u32) {
    spot_in(id, SIDE, frames, 16..W - SIDE - 16, 16..H - SIDE - 16)
}

/// A spot where a 128 px marker for `id` misses the cutoff on `frame`.
pub fn miss_spot(id: u16, frame: u64) -> (u32, u32) {
    for y in (16..H - SIDE - 16).step_by(16) {
        for x in (16..W - SIDE - 16).step_by(16) {
            if !passes(id, (x, y), SIDE, frame) {
                return (x, y);
            }
        }
    }
    panic!("no missing spot");
}

pub fn frames(range: std::ops::Range<u64>) -> Vec<u64> {
    range.collect()
}

/// 640x480 with a 128 px marker for `id` at `at`.
pub fn scene_at(id: u16, at: (u32, u32)) -> RgbImage {
    let mut image = blank(W, H);
    place(&mut image, id, BIG, at);
    image
}

/// 640x480 with a 128 px marker for `id`, placed where it passes on `frames`.
pub fn scene(id: u16, frames: &[u64]) -> (RgbImage, (u32, u32)) {
    let at = spot(id, frames);
    (scene_at(id, at), at)
}

/// The missing and the wanted marker, both 128 px, both passing on frame 0.
pub fn both_classes() -> RgbImage {
    let left = spot_in(MISSING_ID, SIDE, &[0], 16..160, 16..320);
    let right = spot_in(WANTED_ID, SIDE, &[0], 320..480, 16..320);
    let mut image = blank(W, H);
    place(&mut image, MISSING_ID, BIG, left);
    place(&mut image, WANTED_ID, BIG, right);
    image
}

/// `count` frames. Frames in `on` show `picture`; the rest are plain. Each frame
/// has its own dark pixel in the top row, so no encoder stores one as a repeat.
pub fn sequence(count: u32, on: std::ops::Range<u32>, picture: &RgbImage) -> Vec<RgbImage> {
    (0..count)
        .map(|i| {
            if on.contains(&i) {
                let mut frame = picture.clone();
                frame.put_pixel(i % frame.width(), 0, Rgb([0, 0, 0]));
                frame
            } else {
                plain(i)
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// FFmpeg.

/// Run `ffmpeg -y -v error <args>` and require success.
pub fn ffmpeg<I, S>(args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("ffmpeg").args(["-y", "-v", "error"]).args(args).output().expect("ffmpeg runs");
    assert!(output.status.success(), "ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
}

/// Write `frames` as numbered PNGs in a new folder and return the `%03d` pattern.
pub fn png_frames(dir: &Path, frames: &[RgbImage]) -> PathBuf {
    let n = fs::read_dir(dir).map(|d| d.count()).unwrap_or(0);
    let folder = dir.join(format!("frames-{n}"));
    fs::create_dir_all(&folder).unwrap();
    for (i, frame) in frames.iter().enumerate() {
        frame.save(folder.join(format!("f{i:03}.png"))).unwrap();
    }
    folder.join("f%03d.png")
}

/// Encode `frames` at `fps` with these output options. The pictures go to FFmpeg
/// as raw RGB on its input, so nothing is compressed twice.
pub fn encode(_dir: &Path, frames: &[RgbImage], fps: u32, options: &[&str], out: &Path) {
    use std::io::Write;
    let (w, h) = frames[0].dimensions();
    assert!(frames.iter().all(|f| f.dimensions() == (w, h)), "one size per clip");
    let size = format!("{w}x{h}");
    let mut child = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &size, "-framerate", &fps.to_string(), "-i", "-"])
        .args(["-frames:v", &frames.len().to_string(), "-an"])
        .args(options)
        .arg(out)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("ffmpeg runs");
    let mut stdin = child.stdin.take().unwrap();
    for frame in frames {
        stdin.write_all(frame.as_raw()).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
}

/// H.264 at the default quality, as a phone would store it.
pub const H264: &[&str] = &["-c:v", "libx264", "-pix_fmt", "yuv420p"];
/// One picture per keyframe, so a timestamp edit keeps every picture.
pub const H264_INTRA: &[&str] = &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "1", "-bf", "0"];
pub const FFV1: &[&str] = &["-c:v", "ffv1"];

/// A copy of `src` with these extra options, streams copied.
pub fn remux(src: &Path, options: &[&str], out: &Path) {
    let mut args: Vec<&OsStr> = vec!["-i".as_ref(), src.as_os_str()];
    args.extend(options.iter().map(OsStr::new));
    args.push(out.as_os_str());
    ffmpeg(args);
}

/// `setts` that adds `extra` timebase ticks to every picture from `from` on: a hold before `from`.
pub fn hold_before(from: u32, extra: u32) -> String {
    format!("setts=pts='if(lt(N\\,{from}),PTS,PTS+{extra})':dts='if(lt(N\\,{from}),DTS,DTS+{extra})'")
}

// ---------------------------------------------------------------------------
// Stills with tags.

pub fn jpeg_bytes(image: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 100)
        .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
        .unwrap();
    out
}

pub fn png_bytes(image: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
        .unwrap();
    out
}

pub fn webp_bytes(image: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut out)
        .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
        .unwrap();
    out
}

/// A little-endian TIFF header with one IFD holding only the orientation tag.
fn exif_orientation_le(orientation: u8) -> Vec<u8> {
    let mut tiff = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00];
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&u16::from(orientation).to_le_bytes());
    tiff.extend_from_slice(&0u16.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff
}

pub fn jpeg_with_orientation(image: &RgbImage, orientation: u8) -> Vec<u8> {
    let encoded = jpeg_bytes(image);
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&exif_orientation_le(orientation));
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&encoded[2..]);
    out
}

pub fn png_crc(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

pub fn png_chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut typed = tag.to_vec();
    typed.extend_from_slice(body);
    let mut chunk = (body.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(&typed);
    chunk.extend_from_slice(&png_crc(&typed).to_be_bytes());
    chunk
}

/// An `eXIf` chunk right after the header. That is before the first image data,
/// and before the animation control chunk of an animated PNG.
pub fn png_with_orientation(png: &[u8], orientation: u8) -> Vec<u8> {
    let chunk = png_chunk(b"eXIf", &exif_orientation_le(orientation));
    let ihdr_len = u32::from_be_bytes(png[8..12].try_into().unwrap()) as usize;
    let at = 8 + 12 + ihdr_len;
    let mut out = png[..at].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[at..]);
    out
}

pub fn webp_with_orientation(image: &RgbImage, orientation: u8) -> Vec<u8> {
    let encoded = webp_bytes(image);
    let mut chunks = Vec::new();
    let mut index = 12usize;
    while index + 8 <= encoded.len() {
        let tag = encoded[index..index + 4].to_vec();
        let size = u32::from_le_bytes(encoded[index + 4..index + 8].try_into().unwrap()) as usize;
        chunks.push((tag, encoded[index + 8..index + 8 + size].to_vec()));
        index += 8 + size + (size & 1);
    }
    let mut tiff = vec![0x4D, 0x4D, 0x00, 0x2A];
    tiff.extend_from_slice(&8u32.to_be_bytes());
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&0x0112u16.to_be_bytes());
    tiff.extend_from_slice(&3u16.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&u16::from(orientation).to_be_bytes());
    tiff.extend_from_slice(&0u16.to_be_bytes());
    let (w, h) = (image.width() - 1, image.height() - 1);
    let vp8x = vec![0x08, 0, 0, 0, w as u8, (w >> 8) as u8, (w >> 16) as u8, h as u8, (h >> 8) as u8, (h >> 16) as u8];
    chunks.retain(|(tag, _)| tag.as_slice() != b"VP8X" && tag.as_slice() != b"EXIF");
    chunks.insert(0, (b"VP8X".to_vec(), vp8x));
    chunks.push((b"EXIF".to_vec(), tiff));
    let mut body = b"WEBP".to_vec();
    for (tag, payload) in chunks {
        body.extend_from_slice(&tag);
        body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        body.extend_from_slice(&payload);
        if payload.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

// ---------------------------------------------------------------------------
// TIFF.

/// One uncompressed TIFF page.
pub struct TiffPage {
    pub width: u32,
    pub height: u32,
    pub photometric: u16,
    pub bits: Vec<u16>,
    pub data: Vec<u8>,
    pub orientation: u16,
    /// More (tag, SHORT values) entries, such as a color map.
    pub extra: Vec<(u16, Vec<u16>)>,
}

impl TiffPage {
    pub fn rgb(image: &RgbImage, orientation: u16) -> Self {
        Self { width: image.width(), height: image.height(), photometric: 2, bits: vec![8; 3], data: image.as_raw().clone(), orientation, extra: vec![] }
    }

    /// 16-bit RGB: each 8-bit sample in the high byte.
    pub fn rgb16(image: &RgbImage, orientation: u16) -> Self {
        let data = image.as_raw().iter().flat_map(|b| (u16::from(*b) << 8).to_le_bytes()).collect();
        Self { width: image.width(), height: image.height(), photometric: 2, bits: vec![16; 3], data, orientation, extra: vec![] }
    }

    /// CMYK ink: zero for none. The black plate stays empty.
    pub fn cmyk(image: &RgbImage, orientation: u16) -> Self {
        let data = image.pixels().flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], 0]).collect();
        Self { width: image.width(), height: image.height(), photometric: 5, bits: vec![8; 4], data, orientation, extra: vec![] }
    }

    /// Full-resolution studio-range YCbCr (BT.601).
    pub fn ycbcr(image: &RgbImage, orientation: u16) -> Self {
        let data = image
            .pixels()
            .flat_map(|p| {
                let (r, g, b) = (f32::from(p[0]), f32::from(p[1]), f32::from(p[2]));
                let y = 16.0 + (65.481 * r + 128.553 * g + 24.966 * b) / 255.0;
                let cb = 128.0 + (-37.797 * r - 74.203 * g + 112.0 * b) / 255.0;
                let cr = 128.0 + (112.0 * r - 93.786 * g - 18.214 * b) / 255.0;
                [y.round() as u8, cb.round() as u8, cr.round() as u8]
            })
            .collect();
        Self {
            width: image.width(),
            height: image.height(),
            photometric: 6,
            bits: vec![8; 3],
            data,
            orientation,
            extra: vec![(530, vec![1, 1])],
        }
    }

    /// An 8-bit palette page of a black-and-white picture: index 0 black, 1 white.
    pub fn palette(image: &RgbImage, orientation: u16) -> Self {
        let data = image.pixels().map(|p| u8::from(p[0] > 127)).collect();
        let mut map = vec![0u16; 256 * 3];
        for plane in 0..3 {
            map[plane * 256 + 1] = 65535;
        }
        Self { width: image.width(), height: image.height(), photometric: 3, bits: vec![8], data, orientation, extra: vec![(320, map)] }
    }
}

/// Little-endian pages, each with its own directory, values, and pixels.
pub fn tiff(pages: &[TiffPage]) -> Vec<u8> {
    let mut out = vec![0x49, 0x49, 0x2A, 0x00, 0, 0, 0, 0];
    let mut link_at = 4usize;
    for page in pages {
        // (tag, type, values). Type 3 is SHORT and 4 is LONG.
        let mut entries: Vec<(u16, u16, Vec<u32>)> = vec![
            (256, 4, vec![page.width]),
            (257, 4, vec![page.height]),
            (258, 3, page.bits.iter().map(|b| u32::from(*b)).collect()),
            (259, 3, vec![1]),
            (262, 3, vec![u32::from(page.photometric)]),
            (273, 4, vec![0]),
            (274, 3, vec![u32::from(page.orientation)]),
            (277, 3, vec![page.bits.len() as u32]),
            (278, 4, vec![page.height]),
            (279, 4, vec![page.data.len() as u32]),
        ];
        for (tag, values) in &page.extra {
            entries.push((*tag, 3, values.iter().map(|v| u32::from(*v)).collect()));
        }
        entries.sort_by_key(|e| e.0);
        let ifd = out.len();
        out[link_at..link_at + 4].copy_from_slice(&(ifd as u32).to_le_bytes());
        let ifd_len = 2 + entries.len() * 12 + 4;
        out.resize(ifd + ifd_len, 0);
        out[ifd..ifd + 2].copy_from_slice(&(entries.len() as u16).to_le_bytes());
        let mut strip_slot = 0;
        for (i, (tag, kind, values)) in entries.iter().enumerate() {
            let at = ifd + 2 + i * 12;
            out[at..at + 2].copy_from_slice(&tag.to_le_bytes());
            out[at + 2..at + 4].copy_from_slice(&kind.to_le_bytes());
            out[at + 4..at + 8].copy_from_slice(&(values.len() as u32).to_le_bytes());
            let size = if *kind == 3 { 2 } else { 4 };
            let bytes: Vec<u8> = values
                .iter()
                .flat_map(|v| if *kind == 3 { (*v as u16).to_le_bytes().to_vec() } else { v.to_le_bytes().to_vec() })
                .collect();
            if values.len() * size <= 4 {
                out[at + 8..at + 8 + bytes.len()].copy_from_slice(&bytes);
            } else {
                let offset = out.len() as u32;
                out.extend_from_slice(&bytes);
                if out.len() % 2 == 1 {
                    out.push(0);
                }
                out[at + 8..at + 12].copy_from_slice(&offset.to_le_bytes());
            }
            if *tag == 273 {
                strip_slot = at + 8;
            }
        }
        link_at = ifd + 2 + entries.len() * 12;
        let pixels = out.len() as u32;
        out[strip_slot..strip_slot + 4].copy_from_slice(&pixels.to_le_bytes());
        out.extend_from_slice(&page.data);
        if out.len() % 2 == 1 {
            out.push(0);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Motion photos.

pub fn xmp_packet(description: &str) -> String {
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         {description}\n\
         </rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>"
    )
}

/// A Google camera directory: the primary still, then a video item of `video_len` bytes.
pub fn motion_directory(flag: &str, mime: &str, video_len: usize, extra_attrs: &str) -> String {
    xmp_packet(&format!(
        "<rdf:Description rdf:about=\"\" \
         xmlns:Container=\"http://ns.google.com/photos/1.0/container/\" \
         xmlns:Item=\"http://ns.google.com/photos/1.0/container/item/\" \
         xmlns:GCamera=\"http://ns.google.com/photos/1.0/camera/\" \
         GCamera:MotionPhoto=\"{flag}\" GCamera:MotionPhotoVersion=\"1\" {extra_attrs}>\
         <Container:Directory><rdf:Seq>\
         <rdf:li rdf:parseType=\"Resource\">\
         <Container:Item Item:Mime=\"{mime}\" Item:Semantic=\"Primary\" Item:Length=\"0\" Item:Padding=\"0\"/>\
         </rdf:li>\
         <rdf:li rdf:parseType=\"Resource\">\
         <Container:Item Item:Mime=\"video/mp4\" Item:Semantic=\"MotionPhoto\" Item:Length=\"{video_len}\" Item:Padding=\"0\"/>\
         </rdf:li></rdf:Seq></Container:Directory>\
         </rdf:Description>"
    ))
}

/// A JPEG with an XMP APP1 segment right after SOI, then `trailer` after the image.
pub fn jpeg_with_xmp(jpg: &[u8], xmp: &[u8], trailer: &[u8]) -> Vec<u8> {
    assert_eq!(&jpg[..2], b"\xff\xd8");
    let mut payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    payload.extend_from_slice(xmp);
    let mut bytes = jpg[..2].to_vec();
    bytes.extend_from_slice(&[0xFF, 0xE1]);
    bytes.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&jpg[2..]);
    bytes.extend_from_slice(trailer);
    bytes
}

/// Samsung's trailer: the embedded-video marker, the clip, then an SEFH directory and SEFT.
pub fn samsung_trailer(video: &[u8]) -> Vec<u8> {
    let marker = b"\x00\x00\x30\x0A";
    let mut trailer = marker.to_vec();
    trailer.extend_from_slice(&16u32.to_le_bytes());
    trailer.extend_from_slice(b"MotionPhoto_Data");
    trailer.extend_from_slice(video);
    let sefh = trailer.len();
    trailer.extend_from_slice(b"SEFH");
    trailer.extend_from_slice(&106u32.to_le_bytes());
    trailer.extend_from_slice(&1u32.to_le_bytes());
    trailer.extend_from_slice(marker);
    trailer.extend_from_slice(&(sefh as u32).to_le_bytes());
    trailer.extend_from_slice(&(sefh as u32).to_le_bytes());
    let size = (trailer.len() - sefh) as u32;
    trailer.extend_from_slice(&size.to_le_bytes());
    trailer.extend_from_slice(b"SEFT");
    trailer
}

/// An AVIF motion photo: the still, an XMP `uuid` box, then the clip in an `mpvd` box.
pub fn avif_motion(still: &[u8], video: &[u8]) -> Vec<u8> {
    let xmp = motion_directory("1", "image/avif", video.len(), "");
    let uuid = [0xBEu8, 0x7A, 0xCF, 0xCB, 0x97, 0xA9, 0x42, 0xE8, 0x9C, 0x71, 0x99, 0x94, 0x91, 0xE3, 0xAF, 0xAC];
    let mut bytes = still.to_vec();
    bytes.extend_from_slice(&((8 + uuid.len() + xmp.len()) as u32).to_be_bytes());
    bytes.extend_from_slice(b"uuid");
    bytes.extend_from_slice(&uuid);
    bytes.extend_from_slice(xmp.as_bytes());
    bytes.extend_from_slice(&((8 + video.len()) as u32).to_be_bytes());
    bytes.extend_from_slice(b"mpvd");
    bytes.extend_from_slice(video);
    bytes
}

// ---------------------------------------------------------------------------
// The engine, fed directly, for the decision rules.

pub struct Feed {
    pub coverage: Coverage,
    pub fps: f64,
    pub video: bool,
    pub missing: bool,
    pub wanted: bool,
    pub warnings: Vec<Line>,
    pub on_screen: Option<Vec<u64>>,
    pub stop: StopFlag,
    /// Stop with this flag before pushing frame N.
    pub stop_before: Option<(u64, openworld_core::StopReason)>,
}

impl Default for Feed {
    fn default() -> Self {
        Self {
            coverage: Coverage::Complete,
            fps: 30.0,
            video: true,
            missing: true,
            wanted: true,
            warnings: Vec::new(),
            on_screen: None,
            stop: StopFlag::default(),
            stop_before: None,
        }
    }
}

/// Push `frames` in order through an engine reading demo markers. Frames the
/// coverage skips are skipped, as a phone does after asking `wants`.
pub fn feed(frames: &[RgbImage], opts: Feed) -> ScanReport {
    feed_with(frames, opts, Box::new(Markers))
}

/// `feed`, with another way of reading the frames.
pub fn feed_with(frames: &[RgbImage], opts: Feed, perception: Box<dyn openworld_core::perception::Perception + Send>) -> ScanReport {
    let pack_dir = tempfile::tempdir().unwrap();
    let pack = write_fixture_pack(pack_dir.path(), SystemTime::now()).unwrap();
    let cfg = EngineConfig {
        coverage: opts.coverage,
        missing: opts.missing,
        wanted: opts.wanted,
        fps: opts.fps,
        video: opts.video,
        on_screen: opts.on_screen,
        cover_index: None,
        warnings: opts.warnings,
        out_dir: None,
        stop: opts.stop.clone(),
        now: SystemTime::now(),
    };
    let mut engine = Engine::new(bundle("fast"), pack, cfg, perception).expect("demo vectors are valid");
    let mut events = |_: Event| {};
    for (index, frame) in frames.iter().enumerate() {
        let index = index as u64;
        if let Some((at, reason)) = opts.stop_before {
            if at == index {
                opts.stop.stop(reason);
            }
        }
        let keep = if engine.wants(index) { engine.push(index, frame, &mut events) } else { engine.skip(index) };
        if !keep {
            break;
        }
    }
    engine.finish(DecodeEnd { clean: true, declared_frames: Some(frames.len() as u64), reason: None }, &mut events)
}

// ---------------------------------------------------------------------------
// Reading a report.

pub fn describe(r: &ScanReport) -> String {
    let tracks: Vec<String> = r
        .tracks
        .iter()
        .map(|t| {
            format!(
                "#{} frames {}-{} {:?} {}px q{} {}",
                t.track_id,
                t.first_frame,
                t.last_frame,
                t.status,
                t.face_px,
                t.quality_frames,
                t.reason.as_ref().map(|l| l.code.as_str()).unwrap_or("")
            )
        })
        .collect();
    format!(
        "status {:?} headline {} reason {:?} refusal {:?} decoded {} analyzed {} declared {:?} tracks {:?} candidates {:?}",
        r.status,
        r.headline.code,
        r.reason.as_ref().map(|l| &l.text),
        r.refusal,
        r.counts.frames_decoded,
        r.counts.frames_analyzed,
        r.counts.frames_declared,
        tracks,
        r.candidates.iter().map(|c| (&c.poster.id, c.frame_index)).collect::<Vec<_>>()
    )
}

pub fn codes(lines: &[Line]) -> Vec<&str> {
    lines.iter().map(|l| l.code.as_str()).collect()
}

pub fn assert_complete(r: &ScanReport) {
    assert_eq!(r.status, Status::Complete, "{}", describe(r));
    assert!(r.reason.is_none(), "{}", describe(r));
    assert!(r.refusal.is_none(), "{}", describe(r));
}

pub fn assert_refused(r: &ScanReport, code: Copy) {
    assert_eq!(r.status, Status::Refused, "{}", describe(r));
    assert_eq!(r.refusal.as_deref(), Some(code.0), "{}", describe(r));
    assert!(r.headline.is(code), "{}", describe(r));
    assert_not_a_clearance(r);
    assert!(r.tracks.is_empty() && r.candidates.is_empty(), "{}", describe(r));
    assert!(r.frames_line.is_none(), "a refusal decoded nothing: {}", describe(r));
}

pub fn assert_incomplete(r: &ScanReport, reason: Copy) {
    assert_eq!(r.status, Status::Incomplete, "{}", describe(r));
    assert!(r.headline.is(copy::INCOMPLETE), "{}", describe(r));
    assert_eq!(r.reason.as_ref().map(|l| l.code.as_str()), Some(reason.0), "{}", describe(r));
    assert_not_a_clearance(r);
}

/// The clearance sentence is off, and the headline is not "no candidate".
pub fn assert_not_a_clearance(r: &ScanReport) {
    assert!(!r.headline.is(copy::NO_CLEARANCE), "{}", describe(r));
    assert!(r.disclosure.iter().all(|l| !l.is(copy::NO_CLEARANCE)), "{}", describe(r));
    assert!(r.disclosure.iter().any(|l| l.is(copy::NOTHING_UPLOADED)), "{}", describe(r));
}

/// Finished with nothing past the cutoff. Not a clearance: the sentence says so.
pub fn assert_no_candidate(r: &ScanReport) {
    assert_complete(r);
    assert!(r.headline.is(copy::NO_CLEARANCE), "{}", describe(r));
    assert!(r.candidates.is_empty(), "{}", describe(r));
    assert!(r.tracks.iter().all(|t| !matches!(t.status, TrackStatus::Candidate | TrackStatus::TooBrief)), "{}", describe(r));
}

pub fn tracks_with(r: &ScanReport, status: TrackStatus) -> Vec<&TrackItem> {
    r.tracks.iter().filter(|t| t.status == status).collect()
}

/// The finished scan has a candidate card for `poster` from a candidate track.
pub fn assert_candidate<'a>(r: &'a ScanReport, poster: &str) -> &'a Candidate {
    assert_complete(r);
    assert!(r.headline.is(copy::POSSIBLE_CANDIDATE), "{}", describe(r));
    let card = r.candidates.iter().find(|c| c.poster.id == poster).unwrap_or_else(|| panic!("no card for {poster}: {}", describe(r)));
    let track = r.tracks.iter().find(|t| t.track_id == card.track_id).expect("the card's track");
    assert_eq!(track.status, TrackStatus::Candidate, "{}", describe(r));
    assert!(track.face_px >= 112, "{}", describe(r));
    card
}

/// A track starting at `first_frame` was decoded, compared, and passed the cutoff:
/// a candidate, or a video face too brief to be one. A brief one is not a clearance.
pub fn assert_passed_at(r: &ScanReport, first_frame: u64) -> &TrackItem {
    assert_complete(r);
    let track = r
        .tracks
        .iter()
        .find(|t| t.first_frame == first_frame && matches!(t.status, TrackStatus::Candidate | TrackStatus::TooBrief))
        .unwrap_or_else(|| panic!("no passing track from frame {first_frame}: {}", describe(r)));
    match track.status {
        TrackStatus::TooBrief => {
            assert!(track.label.is(copy::TOO_BRIEF));
            assert!(r.candidates.iter().all(|c| c.track_id != track.track_id), "{}", describe(r));
        }
        _ => {
            assert!(track.label.is(copy::POSSIBLE_CANDIDATE));
            assert!(r.candidates.iter().any(|c| c.track_id == track.track_id && c.poster.id == MISSING_POSTER), "{}", describe(r));
        }
    }
    track
}
