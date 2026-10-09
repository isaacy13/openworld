// SPDX-License-Identifier: Apache-2.0

//! Desktop file scans use FFmpeg for video and the image crate for stills.
//! A JPEG is turned to match its camera orientation tag before the pixels are
//! scanned. Audio is not decoded. Phone, Mac, and Android shells decode with
//! AVFoundation or MediaCodec and pass the frames through `load_frame_dir`.
//! That path does not run FFmpeg.

use crate::timeutil::parse_rfc3339;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbImage};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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
    /// True when the container reported a frame count. An estimate from duration is not exact.
    pub frames_exact: bool,
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
        let image = open_oriented(path).map_err(|_| MediaError::BadCodec)?;
        let keep_going = on_frame(0, &image);
        return Ok(DecodeStats { frames_decoded: 1, clean: keep_going, probe: info });
    }
    decode_video(path, &info, &mut on_frame)
}

fn probe_image(path: &Path) -> Option<Probe> {
    // An APNG is a moving picture. The still decoder would keep the first frame and
    // could clear a file it did not finish. FFmpeg decodes every frame.
    if animated_png(path) {
        return None;
    }
    let rgb = open_oriented(path).ok()?;
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
        frames_exact: true,
        container_created: None,
        video: false,
    })
}

/// Decodes a still and applies its camera orientation tag.
/// A PNG has no such tag, so its pixels stay as stored.
fn open_oriented(path: &Path) -> image::ImageResult<RgbImage> {
    let mut decoder = ImageReader::open(path)?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image.to_rgb8())
}

/// True when a PNG carries an animation control chunk before the first image data.
fn animated_png(path: &Path) -> bool {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut signature = [0u8; 8];
    if file.read_exact(&mut signature).is_err() {
        return false;
    }
    if &signature != b"\x89PNG\r\n\x1a\n" {
        return false;
    }
    loop {
        let mut header = [0u8; 8];
        if file.read_exact(&mut header).is_err() {
            return false;
        }
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let tag = &header[4..8];
        if tag == b"acTL" {
            return true;
        }
        if tag == b"IDAT" || tag == b"IEND" {
            return false;
        }
        if file.seek(SeekFrom::Current(len as i64 + 4)).is_err() {
            return false;
        }
    }
}

fn probe_video(path: &Path) -> Result<Probe, MediaError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate,codec_name,nb_frames:stream_side_data=rotation",
            "-show_entries",
            "format=duration:format_tags=creation_time",
            "-show_frames",
            "-read_intervals",
            "%+#1",
            "-show_entries",
            "frame_side_data=rotation",
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
    // FFmpeg turns the frames to the displayed orientation. A quarter turn swaps
    // the stored width and height. The buffer has to match those displayed pixels.
    let (width, height) = if swaps_axes(display_rotation(&value)) {
        (height, width)
    } else {
        (width, height)
    };
    let frames = nb.unwrap_or_else(|| {
        if duration > 0.0 {
            (duration * fps).round() as u64
        } else {
            0
        }
    });
    Ok(Probe {
        width,
        height,
        fps,
        duration_sec: duration,
        frames,
        frames_exact: nb.is_some(),
        container_created: created,
        video: true,
    })
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
    let mut capped = false;
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
        // A one-frame GIF can make FFmpeg repeat the picture. Those repeats are not frames of the file.
        if info.frames_exact && info.frames > 0 && decoded >= info.frames {
            capped = true;
            let _ = child.kill();
            break;
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
    let status = if stopped || capped {
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

/// Rotation from the track matrix or from a display-orientation message, in degrees.
fn display_rotation(value: &Value) -> f64 {
    if let Some(rotation) = side_rotation(value.get("streams").and_then(|s| s.get(0))) {
        return rotation;
    }
    side_rotation(value.get("frames").and_then(|s| s.get(0))).unwrap_or(0.0)
}

fn side_rotation(node: Option<&Value>) -> Option<f64> {
    let list = node?.get("side_data_list")?.as_array()?;
    for item in list {
        let Some(rotation) = item.get("rotation").and_then(Value::as_f64) else {
            continue;
        };
        if rotation.abs() > 0.5 {
            return Some(rotation);
        }
    }
    None
}

/// A quarter turn or three quarter turns exchange width and height. A half turn does not.
fn swaps_axes(rotation: f64) -> bool {
    let turns = rotation.abs() % 180.0;
    (turns - 90.0).abs() < 1.0
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
        let image = open_oriented(&path).map_err(|_| MediaError::BadCodec)?;
        frames.push(image);
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::open_oriented;
    use image::{DynamicImage, ImageEncoder, RgbImage};
    use std::fs;
    use std::path::PathBuf;

    fn jpeg_with_orientation(image: &RgbImage, orientation: u8) -> Vec<u8> {
        let mut encoded = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 100)
            .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
            .unwrap();
        inject_orientation(&encoded, orientation)
    }

    fn inject_orientation(jpeg: &[u8], orientation: u8) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x0112u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&(u16::from(orientation)).to_le_bytes());
        tiff.extend_from_slice(&0u16.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        let mut payload = Vec::new();
        payload.extend_from_slice(b"Exif\0\0");
        payload.extend_from_slice(&tiff);
        let len = (payload.len() + 2) as u16;
        let mut out = Vec::with_capacity(2 + 2 + 2 + payload.len() + jpeg.len().saturating_sub(2));
        out.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE1]);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("ow-{name}-{}.jpg", std::process::id()));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn jpeg_orientation_six_matches_a_clockwise_quarter_turn() {
        let shown = RgbImage::from_raw(3, 2, vec![10, 0, 0, 20, 0, 0, 30, 0, 0, 40, 0, 0, 50, 0, 0, 60, 0, 0]).unwrap();
        let stored = image::imageops::rotate270(&shown);
        let path = write_temp("orient6", &jpeg_with_orientation(&stored, 6));
        let opened = open_oriented(&path).unwrap();
        let raw = image::open(&path).unwrap().to_rgb8();
        let _ = fs::remove_file(&path);
        let expected = DynamicImage::ImageRgb8(raw).rotate90().to_rgb8();
        assert_eq!(opened.dimensions(), expected.dimensions());
        assert_eq!(opened.as_raw(), expected.as_raw());
        assert_ne!(opened.dimensions(), stored.dimensions());
    }

    #[test]
    fn a_jpeg_without_an_orientation_tag_keeps_the_stored_pixels() {
        let shown = RgbImage::from_raw(2, 2, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]).unwrap();
        let mut encoded = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 100)
            .write_image(shown.as_raw(), shown.width(), shown.height(), image::ExtendedColorType::Rgb8)
            .unwrap();
        let path = write_temp("orient1", &encoded);
        let opened = open_oriented(&path).unwrap();
        let raw = image::open(&path).unwrap().to_rgb8();
        let _ = fs::remove_file(&path);
        assert_eq!(opened.dimensions(), raw.dimensions());
        assert_eq!(opened.as_raw(), raw.as_raw());
    }

    #[test]
    fn a_quarter_turn_swaps_the_displayed_axes() {
        let frame = serde_json::json!({"frames":[{"side_data_list":[{"rotation": -90}]}]});
        assert!(super::swaps_axes(super::display_rotation(&frame)));
        let stream = serde_json::json!({"streams":[{"side_data_list":[{"rotation": 90}]}]});
        assert!(super::swaps_axes(super::display_rotation(&stream)));
        let half = serde_json::json!({"streams":[{"side_data_list":[{"rotation": 180}]}]});
        assert!(!super::swaps_axes(super::display_rotation(&half)));
        let none = serde_json::json!({"streams":[{"side_data_list":[{}]}],"frames":[{"side_data_list":[{}]}]});
        assert!(!super::swaps_axes(super::display_rotation(&none)));
    }
}
