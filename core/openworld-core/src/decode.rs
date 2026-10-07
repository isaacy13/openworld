// SPDX-License-Identifier: Apache-2.0

//! Desktop file scans use FFmpeg for video and the image crate for stills.
//! Audio is not decoded. Phone, Mac, and Android shells decode with
//! AVFoundation or MediaCodec and pass the frames through `load_frame_dir`.
//! That path does not run FFmpeg.

use crate::timeutil::parse_rfc3339;
use image::RgbImage;
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::SystemTime;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("bad codec or unreadable file")]
    BadCodec,
}

#[derive(Clone, Debug)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_sec: f64,
    pub frames: u64,
    pub container_created: Option<SystemTime>,
    pub video: bool,
}

#[derive(Debug)]
pub struct DecodeStats {
    pub frames_decoded: u64,
    pub clean: bool,
    pub probe: Probe,
}

pub fn probe(path: &Path) -> Result<Probe, MediaError> {
    if !path.is_file() {
        return Err(MediaError::BadCodec);
    }
    if let Some(info) = probe_image(path) {
        return Ok(info);
    }
    probe_video(path)
}

pub fn for_each_frame(path: &Path, mut on_frame: impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    let info = probe(path)?;
    if !info.video {
        let image = image::open(path).map_err(|_| MediaError::BadCodec)?.to_rgb8();
        let keep_going = on_frame(0, &image);
        return Ok(DecodeStats { frames_decoded: 1, clean: keep_going, probe: info });
    }
    decode_video(path, &info, &mut on_frame)
}

fn probe_image(path: &Path) -> Option<Probe> {
    let image = image::open(path).ok()?;
    let rgb = image.to_rgb8();
    let (width, height) = rgb.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some(Probe {
        width,
        height,
        fps: 0.0,
        duration_sec: 0.0,
        frames: 1,
        container_created: None,
        video: false,
    })
}

fn probe_video(path: &Path) -> Result<Probe, MediaError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate,codec_name,nb_frames",
            "-show_entries",
            "format=duration:format_tags=creation_time",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|_| MediaError::BadCodec)?;
    if !output.status.success() {
        return Err(MediaError::BadCodec);
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| MediaError::BadCodec)?;
    let stream = value.get("streams").and_then(|s| s.get(0)).ok_or(MediaError::BadCodec)?;
    let width = json_u32(stream.get("width")).ok_or(MediaError::BadCodec)?;
    let height = json_u32(stream.get("height")).ok_or(MediaError::BadCodec)?;
    let fps = stream
        .get("avg_frame_rate")
        .and_then(|v| v.as_str())
        .and_then(parse_ratio)
        .unwrap_or(0.0);
    let nb = json_u32(stream.get("nb_frames")).map(u64::from);
    let duration = value
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let created = value
        .pointer("/format/tags/creation_time")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339);
    if width == 0 || height == 0 || fps <= 0.0 {
        return Err(MediaError::BadCodec);
    }
    let frames = nb.unwrap_or_else(|| {
        if duration > 0.0 {
            (duration * fps).round() as u64
        } else {
            0
        }
    });
    Ok(Probe { width, height, fps, duration_sec: duration, frames, container_created: created, video: true })
}

fn decode_video(path: &Path, info: &Probe, on_frame: &mut impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-an", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| MediaError::BadCodec)?;
    let mut stdout = child.stdout.take().ok_or(MediaError::BadCodec)?;
    let frame_bytes = info.width as usize * info.height as usize * 3;
    if frame_bytes == 0 {
        let _ = child.kill();
        return Err(MediaError::BadCodec);
    }
    let mut decoded = 0u64;
    let mut stopped = false;
    loop {
        let mut buf = vec![0u8; frame_bytes];
        let mut filled = 0;
        while filled < frame_bytes {
            match stdout.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(_) => {
                    let _ = child.kill();
                    return Err(MediaError::BadCodec);
                }
            }
        }
        if filled == 0 {
            break;
        }
        if filled != frame_bytes {
            let _ = child.kill();
            return Err(MediaError::BadCodec);
        }
        let image = RgbImage::from_raw(info.width, info.height, buf).ok_or(MediaError::BadCodec)?;
        let keep = on_frame(decoded, &image);
        decoded += 1;
        if !keep {
            stopped = true;
            let _ = child.kill();
            break;
        }
    }
    let status = if stopped {
        let _ = child.wait();
        true
    } else {
        child.wait().map(|s| s.success()).unwrap_or(false)
    };
    if decoded == 0 || !status && !stopped {
        if decoded == 0 {
            return Err(MediaError::BadCodec);
        }
        return Ok(DecodeStats { frames_decoded: decoded, clean: false, probe: info.clone() });
    }
    Ok(DecodeStats { frames_decoded: decoded, clean: !stopped, probe: info.clone() })
}

fn parse_ratio(text: &str) -> Option<f64> {
    let (num, den) = text.split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    if den == 0.0 {
        None
    } else {
        let fps = num / den;
        if fps > 0.0 && fps < 1000.0 {
            Some(fps)
        } else {
            None
        }
    }
}

fn json_u32(value: Option<&Value>) -> Option<u32> {
    let value = value?;
    if let Some(n) = value.as_u64() {
        return u32::try_from(n).ok();
    }
    if let Some(n) = value.as_i64() {
        return u32::try_from(n).ok();
    }
    if let Some(text) = value.as_str() {
        if text == "N/A" || text.is_empty() {
            return None;
        }
        return text.parse().ok();
    }
    None
}

/// Ordered stills already decoded by a platform shell or a test.
/// Names sort in decode order, so callers pad the index (`frame_000001.png`).
pub fn load_frame_dir(dir: &Path) -> Result<Vec<RgbImage>, MediaError> {
    if !dir.is_dir() {
        return Err(MediaError::BadCodec);
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|_| MediaError::BadCodec)? {
        let path = entry.map_err(|_| MediaError::BadCodec)?.path();
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "png" || ext == "jpg" || ext == "jpeg" {
            names.push(path);
        }
    }
    names.sort();
    if names.is_empty() {
        return Err(MediaError::BadCodec);
    }
    let mut frames = Vec::with_capacity(names.len());
    for path in names {
        let image = image::open(&path).map_err(|_| MediaError::BadCodec)?.to_rgb8();
        frames.push(image);
    }
    Ok(frames)
}
