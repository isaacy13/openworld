// SPDX-License-Identifier: Apache-2.0

//! Desktop file scans use FFmpeg for video and the image crate for stills.
//! A JPEG, a still WebP, a PNG, or a TIFF is turned to match its camera orientation
//! tag before the pixels are scanned. Every page of a TIFF is scanned. A 16-bit
//! sample contributes its high 8 bits. CMYK and full-resolution YCbCr become RGB.
//! A one-page palette, bilevel, or subsampled TIFF is read as a player shows that
//! page. A multi-page TIFF whose pages are not all read is refused. Audio is not decoded. Phone, Mac, and Android shells
//! decode with AVFoundation or MediaCodec and pass the frames through
//! `load_frame_dir`. That path does not run FFmpeg. It reads the container creation
//! time from the file, so a clock that disagrees with the file time warns the same
//! way the desktop probe does.

use crate::timeutil::parse_rfc3339;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbImage};
use serde_json::Value;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
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
    /// The stored pixels are not square. Width and height are the displayed size.
    pub square_pixels: bool,
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
    if is_tiff(path) {
        if let Ok(info) = probe_tiff(path) {
            return Ok(info);
        }
        return probe_tiff_player(path);
    }
    if let Some(info) = probe_image(path) {
        return Ok(info);
    }
    probe_video(path)
}

pub fn for_each_frame(path: &Path, mut on_frame: impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    if is_tiff(path) {
        if let Ok(info) = probe_tiff(path) {
            return decode_tiff(path, info, &mut on_frame);
        }
        return decode_tiff_player(path, &mut on_frame);
    }
    let info = probe(path)?;
    if !info.video {
        let image = open_oriented(path).map_err(|_| MediaError::BadCodec)?;
        let keep_going = on_frame(0, &image);
        return Ok(DecodeStats { frames_decoded: 1, clean: keep_going, probe: info });
    }
    decode_video(path, &info, &mut on_frame)
}

fn is_tiff(path: &Path) -> bool {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut header = [0u8; 4];
    if file.read_exact(&mut header).is_err() {
        return false;
    }
    matches!(header, [0x49, 0x49, 0x2A, 0x00] | [0x4D, 0x4D, 0x00, 0x2A])
}

/// Every page, each turned by that page's orientation tag.
fn probe_tiff(path: &Path) -> Result<Probe, MediaError> {
    let pages = read_tiff_pages(path)?;
    let (width, height) = pages[0].dimensions();
    let count = pages.len() as u64;
    let moving = count > 1;
    Ok(Probe {
        width,
        height,
        fps: if moving { 1.0 } else { 0.0 },
        duration_sec: if moving { count as f64 } else { 0.0 },
        frames: count,
        frames_exact: true,
        container_created: None,
        video: moving,
        square_pixels: false,
    })
}

fn decode_tiff(path: &Path, info: Probe, on_frame: &mut impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    let pages = read_tiff_pages(path)?;
    let mut decoded = 0u64;
    let mut clean = true;
    for page in &pages {
        if !on_frame(decoded, page) {
            decoded += 1;
            clean = false;
            break;
        }
        decoded += 1;
    }
    Ok(DecodeStats { frames_decoded: decoded, clean, probe: info })
}

fn read_tiff_pages(path: &Path) -> Result<Vec<RgbImage>, MediaError> {
    let file = File::open(path).map_err(|_| MediaError::BadCodec)?;
    let mut decoder = tiff::decoder::Decoder::new(BufReader::new(file)).map_err(|_| MediaError::BadCodec)?;
    let mut pages = Vec::new();
    loop {
        pages.push(read_tiff_page(&mut decoder)?);
        if !decoder.more_images() {
            break;
        }
        decoder.next_image().map_err(|_| MediaError::BadCodec)?;
    }
    Ok(pages)
}

fn read_tiff_page<R: Read + Seek>(decoder: &mut tiff::decoder::Decoder<R>) -> Result<RgbImage, MediaError> {
    let (width, height) = decoder.dimensions().map_err(|_| MediaError::BadCodec)?;
    if width == 0 || height == 0 {
        return Err(MediaError::BadCodec);
    }
    let color = decoder.colortype().map_err(|_| MediaError::BadCodec)?;
    let (samples, bits) = match color {
        tiff::ColorType::Gray(bits) if (8..=16).contains(&bits) => (1, bits),
        tiff::ColorType::RGB(bits) if (8..=16).contains(&bits) => (3, bits),
        tiff::ColorType::RGBA(bits) if (8..=16).contains(&bits) => (4, bits),
        tiff::ColorType::CMYK(8) => (4, 8),
        tiff::ColorType::YCbCr(8) => (3, 8),
        _ => return Err(MediaError::BadCodec),
    };
    let tag = decoder
        .find_tag(tiff::tags::Tag::Orientation)
        .ok()
        .flatten()
        .and_then(|value| value.into_u16().ok())
        .map(|value| value as u8)
        .filter(|value| (1..=8).contains(value))
        .unwrap_or(1);
    let decoded = decoder.read_image().map_err(|_| MediaError::BadCodec)?;
    let rgb = match (color, decoded) {
        (tiff::ColorType::CMYK(8), tiff::decoder::DecodingResult::U8(bytes)) => cmyk_to_rgb(&bytes, width, height)?,
        (tiff::ColorType::YCbCr(8), tiff::decoder::DecodingResult::U8(bytes)) => ycbcr_to_rgb(&bytes, width, height)?,
        (_, tiff::decoder::DecodingResult::U8(bytes)) => samples_to_rgb(&bytes, width, height, samples)?,
        (_, tiff::decoder::DecodingResult::U16(samples_wide)) => {
            samples_u16_to_rgb(&samples_wide, width, height, samples, bits)?
        }
        _ => return Err(MediaError::BadCodec),
    };
    let mut image = DynamicImage::ImageRgb8(rgb);
    if let Some(orientation) = Orientation::from_exif(tag) {
        image.apply_orientation(orientation);
    }
    Ok(image.to_rgb8())
}

fn samples_to_rgb(bytes: &[u8], width: u32, height: u32, samples: usize) -> Result<RgbImage, MediaError> {
    let pixels = (width as usize).checked_mul(height as usize).ok_or(MediaError::BadCodec)?;
    let expected = pixels.checked_mul(samples).ok_or(MediaError::BadCodec)?;
    if bytes.len() < expected {
        return Err(MediaError::BadCodec);
    }
    let mut rgb = Vec::with_capacity(pixels * 3);
    match samples {
        1 => {
            for &pixel in bytes.iter().take(pixels) {
                rgb.extend_from_slice(&[pixel, pixel, pixel]);
            }
        }
        3 => rgb.extend_from_slice(&bytes[..expected]),
        4 => {
            for pixel in bytes[..expected].chunks_exact(4) {
                rgb.extend_from_slice(&pixel[..3]);
            }
        }
        _ => return Err(MediaError::BadCodec),
    }
    RgbImage::from_raw(width, height, rgb).ok_or(MediaError::BadCodec)
}

/// Ink values use 0 for none and 255 for full. K is the black plate.
fn cmyk_to_rgb(bytes: &[u8], width: u32, height: u32) -> Result<RgbImage, MediaError> {
    let pixels = (width as usize).checked_mul(height as usize).ok_or(MediaError::BadCodec)?;
    let expected = pixels.checked_mul(4).ok_or(MediaError::BadCodec)?;
    if bytes.len() < expected {
        return Err(MediaError::BadCodec);
    }
    let mut rgb = Vec::with_capacity(pixels * 3);
    for pixel in bytes[..expected].chunks_exact(4) {
        let cyan = u32::from(pixel[0]);
        let magenta = u32::from(pixel[1]);
        let yellow = u32::from(pixel[2]);
        let black = u32::from(pixel[3]);
        let red = (255 - cyan) * (255 - black) / 255;
        let green = (255 - magenta) * (255 - black) / 255;
        let blue = (255 - yellow) * (255 - black) / 255;
        rgb.extend_from_slice(&[red as u8, green as u8, blue as u8]);
    }
    RgbImage::from_raw(width, height, rgb).ok_or(MediaError::BadCodec)
}

/// Studio-range BT.601. Y 16 is black and Y 235 is white. Chroma is centered at 128.
fn ycbcr_to_rgb(bytes: &[u8], width: u32, height: u32) -> Result<RgbImage, MediaError> {
    let pixels = (width as usize).checked_mul(height as usize).ok_or(MediaError::BadCodec)?;
    let expected = pixels.checked_mul(3).ok_or(MediaError::BadCodec)?;
    if bytes.len() < expected {
        return Err(MediaError::BadCodec);
    }
    let mut rgb = Vec::with_capacity(pixels * 3);
    for pixel in bytes[..expected].chunks_exact(3) {
        let y = i32::from(pixel[0]) - 16;
        let cb = i32::from(pixel[1]) - 128;
        let cr = i32::from(pixel[2]) - 128;
        let red = (298 * y + 409 * cr + 128) / 256;
        let green = (298 * y - 100 * cb - 208 * cr + 128) / 256;
        let blue = (298 * y + 516 * cb + 128) / 256;
        rgb.push(red.clamp(0, 255) as u8);
        rgb.push(green.clamp(0, 255) as u8);
        rgb.push(blue.clamp(0, 255) as u8);
    }
    RgbImage::from_raw(width, height, rgb).ok_or(MediaError::BadCodec)
}

/// One page, shown the way a player shows it, when the page reader cannot expand the samples.
fn probe_tiff_player(path: &Path) -> Result<Probe, MediaError> {
    let bytes = std::fs::read(path).map_err(|_| MediaError::BadCodec)?;
    if tiff_page_count(&bytes) != Some(1) {
        return Err(MediaError::BadCodec);
    }
    let (width, height) = tiff_stored_size(&bytes).ok_or(MediaError::BadCodec)?;
    let tag = tiff_orientation_tag(&bytes).unwrap_or(1);
    let (width, height) = oriented_still_size(width, height, tag);
    Ok(Probe {
        width,
        height,
        fps: 0.0,
        duration_sec: 0.0,
        frames: 1,
        frames_exact: true,
        container_created: None,
        video: false,
        square_pixels: false,
    })
}

fn decode_tiff_player(path: &Path, on_frame: &mut impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    let bytes = std::fs::read(path).map_err(|_| MediaError::BadCodec)?;
    if tiff_page_count(&bytes) != Some(1) {
        return Err(MediaError::BadCodec);
    }
    let (width, height) = tiff_stored_size(&bytes).ok_or(MediaError::BadCodec)?;
    let tag = tiff_orientation_tag(&bytes).unwrap_or(1);
    let stored = Probe {
        width,
        height,
        fps: 0.0,
        duration_sec: 0.0,
        frames: 1,
        frames_exact: true,
        container_created: None,
        video: false,
        square_pixels: false,
    };
    let mut frame = None;
    let stats = decode_video(path, &stored, &mut |_, image| {
        frame = Some(image.clone());
        true
    })?;
    if stats.frames_decoded == 0 {
        return Err(MediaError::BadCodec);
    }
    let Some(image) = frame else {
        return Err(MediaError::BadCodec);
    };
    let mut shown = DynamicImage::ImageRgb8(image);
    if let Some(orientation) = Orientation::from_exif(tag) {
        shown.apply_orientation(orientation);
    }
    let shown = shown.to_rgb8();
    let (width, height) = shown.dimensions();
    let keep = on_frame(0, &shown);
    Ok(DecodeStats {
        frames_decoded: 1,
        clean: keep && stats.clean,
        probe: Probe {
            width,
            height,
            fps: 0.0,
            duration_sec: 0.0,
            frames: 1,
            frames_exact: true,
            container_created: None,
            video: false,
            square_pixels: false,
        },
    })
}

fn oriented_still_size(width: u32, height: u32, tag: u8) -> (u32, u32) {
    if matches!(tag, 5 | 6 | 7 | 8) {
        (height, width)
    } else {
        (width, height)
    }
}

fn tiff_page_count(bytes: &[u8]) -> Option<u32> {
    let little = tiff_little(bytes)?;
    let mut ifd = read_u32(bytes, 4, little)? as usize;
    let mut seen = [0usize; 64];
    let mut count = 0u32;
    while ifd != 0 {
        if count as usize >= seen.len() || seen[..count as usize].contains(&ifd) {
            return None;
        }
        seen[count as usize] = ifd;
        if ifd + 2 > bytes.len() {
            return None;
        }
        let entries = read_u16(bytes, ifd, little)? as usize;
        if entries > 64 {
            return None;
        }
        let next = ifd.checked_add(2)?.checked_add(entries.checked_mul(12)?)?;
        if next + 4 > bytes.len() {
            return None;
        }
        ifd = read_u32(bytes, next, little)? as usize;
        count += 1;
    }
    if count == 0 { None } else { Some(count) }
}

fn tiff_stored_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let little = tiff_little(bytes)?;
    let ifd = read_u32(bytes, 4, little)? as usize;
    let width = tiff_ifd_value(bytes, ifd, little, 256)?;
    let height = tiff_ifd_value(bytes, ifd, little, 257)?;
    if width == 0 || height == 0 {
        None
    } else {
        Some((width, height))
    }
}

fn tiff_ifd_value(bytes: &[u8], ifd: usize, little: bool, wanted: u16) -> Option<u32> {
    if ifd + 2 > bytes.len() {
        return None;
    }
    let entries = read_u16(bytes, ifd, little)? as usize;
    if entries > 64 {
        return None;
    }
    let mut cursor = ifd + 2;
    for _ in 0..entries {
        if cursor + 12 > bytes.len() {
            return None;
        }
        let tag = read_u16(bytes, cursor, little)?;
        let format = read_u16(bytes, cursor + 2, little)?;
        let count = read_u32(bytes, cursor + 4, little)?;
        if tag == wanted && count == 1 && (format == 3 || format == 4) {
            return if format == 3 {
                Some(u32::from(read_u16(bytes, cursor + 8, little)?))
            } else {
                read_u32(bytes, cursor + 8, little)
            };
        }
        cursor += 12;
    }
    None
}

fn tiff_little(bytes: &[u8]) -> Option<bool> {
    match bytes.get(0..4)? {
        [0x49, 0x49, 0x2A, 0x00] => Some(true),
        [0x4D, 0x4D, 0x00, 0x2A] => Some(false),
        _ => None,
    }
}

/// The high 8 bits of a 9- to 16-bit sample are the picture that is scanned.
fn samples_u16_to_rgb(
    samples_wide: &[u16],
    width: u32,
    height: u32,
    samples: usize,
    bits: u8,
) -> Result<RgbImage, MediaError> {
    let shift = u32::from(bits.saturating_sub(8));
    let narrow: Vec<u8> = samples_wide.iter().map(|value| (u32::from(*value) >> shift) as u8).collect();
    samples_to_rgb(&narrow, width, height, samples)
}

fn probe_image(path: &Path) -> Option<Probe> {
    // An APNG or an animated WebP is a moving picture. The still decoder would keep
    // the first frame and could clear a file it did not finish. FFmpeg decodes every
    // frame it can. A phone that cannot read every frame refuses before this path.
    if animated_png(path) || animated_webp(path) {
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
        square_pixels: false,
    })
}

/// Decodes a still and applies its camera orientation tag when the file carries one.
fn open_oriented(path: &Path) -> image::ImageResult<RgbImage> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let mut orientation = decoder.orientation()?;
    // The WebP decoder reads a TIFF header at the start of the EXIF chunk.
    // Some files put the JPEG "Exif" marker in front of that header.
    if orientation == Orientation::NoTransforms {
        if let Some(parsed) = webp_exif_orientation(path).or_else(|| png_exif_orientation(path)) {
            orientation = parsed;
        }
    }
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

/// True when a WebP carries an animation chunk or the animation flag.
fn animated_webp(path: &Path) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return false;
    }
    let mut index = 12usize;
    while index + 8 <= bytes.len() {
        let tag = &bytes[index..index + 4];
        let size = u32::from_le_bytes([bytes[index + 4], bytes[index + 5], bytes[index + 6], bytes[index + 7]]) as usize;
        let start = index + 8;
        let end = match start.checked_add(size) {
            Some(end) if end <= bytes.len() => end,
            _ => return false,
        };
        if tag == b"ANIM" || tag == b"ANMF" {
            return true;
        }
        if tag == b"VP8X" && size >= 1 && bytes[start] & 0x02 != 0 {
            return true;
        }
        index = end + (size & 1);
    }
    false
}

/// Orientation from a WebP EXIF chunk. The payload is a TIFF header, sometimes
/// behind the six-byte marker a JPEG uses.
fn webp_exif_orientation(path: &Path) -> Option<Orientation> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let mut index = 12usize;
    while index + 8 <= bytes.len() {
        let tag = &bytes[index..index + 4];
        let size = u32::from_le_bytes([bytes[index + 4], bytes[index + 5], bytes[index + 6], bytes[index + 7]]) as usize;
        let start = index + 8;
        let end = start.checked_add(size)?;
        if end > bytes.len() {
            return None;
        }
        if tag == b"EXIF" {
            return orientation_in_exif(&bytes[start..end]);
        }
        index = end + (size & 1);
    }
    None
}

/// Orientation from a PNG eXIf chunk. The payload is a TIFF header, sometimes
/// behind the six-byte marker a JPEG uses.
fn png_exif_orientation(path: &Path) -> Option<Orientation> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 8 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut index = 8usize;
    while index + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[index..index + 4].try_into().ok()?) as usize;
        let tag = &bytes[index + 4..index + 8];
        let start = index + 8;
        let end = start.checked_add(len)?;
        if end + 4 > bytes.len() {
            return None;
        }
        if tag == b"eXIf" {
            return orientation_in_exif(&bytes[start..end]);
        }
        if tag == b"IEND" {
            return None;
        }
        index = end + 4;
    }
    None
}

fn orientation_in_exif(payload: &[u8]) -> Option<Orientation> {
    let tiff = if payload.len() >= 6 && &payload[..6] == b"Exif\0\0" {
        &payload[6..]
    } else {
        payload
    };
    Orientation::from_exif(tiff_orientation_tag(tiff)?)
}

fn tiff_orientation_tag(tiff: &[u8]) -> Option<u8> {
    if tiff.len() < 8 {
        return None;
    }
    let little = match tiff[0..4] {
        [0x49, 0x49, 0x2A, 0x00] => true,
        [0x4D, 0x4D, 0x00, 0x2A] => false,
        _ => return None,
    };
    let ifd = read_u32(tiff, 4, little)?;
    if ifd < 8 {
        return None;
    }
    let mut cursor = ifd as usize;
    if cursor + 2 > tiff.len() {
        return None;
    }
    let entries = read_u16(tiff, cursor, little)? as usize;
    cursor += 2;
    if entries > 64 {
        return None;
    }
    for _ in 0..entries {
        if cursor + 12 > tiff.len() {
            return None;
        }
        let tag = read_u16(tiff, cursor, little)?;
        let format = read_u16(tiff, cursor + 2, little)?;
        let count = read_u32(tiff, cursor + 4, little)?;
        if tag == 0x0112 && format == 3 && count == 1 {
            let value = read_u16(tiff, cursor + 8, little)?;
            return if (1..=8).contains(&value) { Some(value as u8) } else { Some(1) };
        }
        cursor += 12;
    }
    None
}

fn read_u16(bytes: &[u8], offset: usize, little: bool) -> Option<u16> {
    let pair = bytes.get(offset..offset + 2)?;
    Some(if little {
        u16::from_le_bytes([pair[0], pair[1]])
    } else {
        u16::from_be_bytes([pair[0], pair[1]])
    })
}

fn read_u32(bytes: &[u8], offset: usize, little: bool) -> Option<u32> {
    let quad = bytes.get(offset..offset + 4)?;
    Some(if little {
        u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]])
    } else {
        u32::from_be_bytes([quad[0], quad[1], quad[2], quad[3]])
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
            "stream=width,height,sample_aspect_ratio,avg_frame_rate,codec_name,nb_frames,duration:stream_tags=DURATION:stream_side_data=rotation",
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
    // FFmpeg turns the frames, then stretches non-square pixels. A quarter turn
    // swaps the axes and inverts the sample aspect. The buffer has to match
    // those displayed pixels.
    let turned = swaps_axes(display_rotation(&value));
    let (sar_num, sar_den) = sample_aspect(stream.get("sample_aspect_ratio").and_then(|v| v.as_str()));
    let (width, height, square_pixels) = displayed_size(width, height, sar_num, sar_den, turned);
    // The container duration includes audio that continues after the pictures.
    // Matroska and WebM put the picture length in the track's DURATION tag.
    // MP4 puts it on the video stream. The frame estimate uses that length.
    let picture = picture_duration(stream, duration);
    let presented = if picture > 0.0 { (picture * fps).round() as u64 } else { 0 };
    // nb_frames counts every stored sample. An edit list can hide some of them.
    // The stream duration is the length a player shows.
    let frames = match nb {
        Some(exact) if presented > 0 && presented < exact => presented,
        Some(exact) => exact,
        None => presented,
    };
    Ok(Probe {
        width,
        height,
        fps,
        duration_sec: if picture > 0.0 { picture } else { duration },
        frames,
        frames_exact: nb.is_some(),
        container_created: created,
        video: true,
        square_pixels,
    })
}

/// Picture length in seconds. A track clock wins, then the video stream's own duration.
fn picture_duration(stream: &Value, format_duration: f64) -> f64 {
    if let Some(clock) = stream
        .pointer("/tags/DURATION")
        .and_then(|v| v.as_str())
        .and_then(parse_clock)
    {
        if clock > 0.0 {
            return clock;
        }
    }
    if let Some(text) = stream.get("duration").and_then(|v| v.as_str()) {
        if let Ok(stream_duration) = text.parse::<f64>() {
            if stream_duration > 0.0 {
                return stream_duration;
            }
        }
    }
    format_duration
}

/// `HH:MM:SS.fraction` as written on a Matroska track.
fn parse_clock(text: &str) -> Option<f64> {
    let mut parts = text.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn decode_video(path: &Path, info: &Probe, on_frame: &mut impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    let mut command = Command::new("ffmpeg");
    command.args(["-v", "error", "-i"]).arg(path);
    if info.square_pixels {
        command.args(["-vf", "scale=iw*sar:ih,setsar=1"]);
    }
    let mut child = command
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

/// `N/A`, missing, and `1:1` are square pixels. Anything else is a display stretch.
fn sample_aspect(text: Option<&str>) -> (u32, u32) {
    let Some(text) = text else {
        return (1, 1);
    };
    if text.is_empty() || text == "N/A" {
        return (1, 1);
    }
    let Some((num, den)) = text.split_once(':') else {
        return (1, 1);
    };
    let Ok(num) = num.parse::<u32>() else {
        return (1, 1);
    };
    let Ok(den) = den.parse::<u32>() else {
        return (1, 1);
    };
    if num == 0 || den == 0 {
        (1, 1)
    } else {
        (num, den)
    }
}

/// Display size after a quarter turn and a sample-aspect stretch, in that order.
fn displayed_size(width: u32, height: u32, sar_num: u32, sar_den: u32, turned: bool) -> (u32, u32, bool) {
    if turned && sar_num != sar_den {
        (square_width(height, sar_den, sar_num), width, true)
    } else if turned {
        (height, width, false)
    } else if sar_num != sar_den {
        (square_width(width, sar_num, sar_den), height, true)
    } else {
        (width, height, false)
    }
}

fn square_width(width: u32, num: u32, den: u32) -> u32 {
    let wide = (u64::from(width) * u64::from(num) + u64::from(den) / 2) / u64::from(den);
    u32::try_from(wide).unwrap_or(width).max(1)
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

/// The container clock the desktop probe reports as `creation_time`. A still has none.
pub fn container_created(path: &Path) -> Option<SystemTime> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 12];
    if file.read_exact(&mut header).is_err() {
        return None;
    }
    if &header[4..8] == b"ftyp" {
        return mp4_created(&mut file);
    }
    if header[..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        return matroska_created(&mut file);
    }
    None
}

fn mp4_created(file: &mut File) -> Option<SystemTime> {
    let len = file.metadata().ok()?.len();
    let (moov, moov_end) = atom_body(file, 0, len, b"moov")?;
    let (body, _) = atom_body(file, moov, moov_end, b"mvhd")?;
    file.seek(SeekFrom::Start(body)).ok()?;
    let mut version = [0u8; 4];
    file.read_exact(&mut version).ok()?;
    let created = match version[0] {
        0 => {
            let mut bytes = [0u8; 4];
            file.read_exact(&mut bytes).ok()?;
            u64::from(u32::from_be_bytes(bytes))
        }
        1 => {
            let mut bytes = [0u8; 8];
            file.read_exact(&mut bytes).ok()?;
            u64::from_be_bytes(bytes)
        }
        _ => return None,
    };
    // QuickTime counts seconds from 1904-01-01. Zero means the file has no clock.
    if created == 0 {
        return None;
    }
    let unix = created.checked_sub(2_082_844_800)?;
    Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(unix))
}

fn atom_body(file: &mut File, start: u64, end: u64, name: &[u8; 4]) -> Option<(u64, u64)> {
    let mut pos = start;
    while pos + 8 <= end {
        file.seek(SeekFrom::Start(pos)).ok()?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header).ok()?;
        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let (header_len, size) = if size32 == 1 {
            let mut wide = [0u8; 8];
            file.read_exact(&mut wide).ok()?;
            (16u64, u64::from_be_bytes(wide))
        } else if size32 == 0 {
            (8u64, end.saturating_sub(pos))
        } else {
            (8u64, u64::from(size32))
        };
        if size < header_len || pos.saturating_add(size) > end {
            return None;
        }
        let body = pos + header_len;
        if &header[4..8] == name {
            return Some((body, pos + size));
        }
        pos += size;
    }
    None
}

fn matroska_created(file: &mut File) -> Option<SystemTime> {
    let len = file.metadata().ok()?.len().min(4 * 1024 * 1024);
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut data = vec![0u8; len as usize];
    file.read_exact(&mut data).ok()?;
    let ns = dateutc(&data, 0, data.len())?;
    let secs = ns.div_euclid(1_000_000_000);
    // DateUTC counts nanoseconds from 2001-01-01.
    let unix = secs.checked_add(978_307_200)?;
    if unix < 0 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(unix as u64))
}

fn dateutc(data: &[u8], mut pos: usize, end: usize) -> Option<i64> {
    let end = end.min(data.len());
    while pos + 2 <= end {
        let (id, id_len) = ebml_id(data, pos)?;
        let (size, size_len, unknown) = ebml_size(data, pos + id_len)?;
        let body = pos + id_len + size_len;
        if body > end {
            return None;
        }
        if id == 0x1F43B675 {
            return None;
        }
        if id == 0x4461 {
            if size == 8 && body + 8 <= end {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&data[body..body + 8]);
                return Some(i64::from_be_bytes(bytes));
            }
            return None;
        }
        let child_end = if unknown {
            end
        } else {
            body.saturating_add(size as usize).min(end)
        };
        if matches!(id, 0x1A45DFA3 | 0x18538067 | 0x1549A966) {
            if let Some(ns) = dateutc(data, body, child_end) {
                return Some(ns);
            }
        }
        if unknown || child_end <= pos {
            return None;
        }
        pos = child_end;
    }
    None
}

fn ebml_id(data: &[u8], pos: usize) -> Option<(u64, usize)> {
    let first = *data.get(pos)?;
    let len = first.leading_zeros() as usize + 1;
    if len > 4 || pos + len > data.len() {
        return None;
    }
    let mut id = 0u64;
    for byte in &data[pos..pos + len] {
        id = (id << 8) | u64::from(*byte);
    }
    Some((id, len))
}

fn ebml_size(data: &[u8], pos: usize) -> Option<(u64, usize, bool)> {
    let first = *data.get(pos)?;
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 8 || pos + len > data.len() {
        return None;
    }
    // An eight-byte size keeps its marker in the top bit and has no data bits there.
    let mask = if len == 8 { 0 } else { 0xFFu8 >> len };
    let mut value = u64::from(first & mask);
    for byte in &data[pos + 1..pos + len] {
        value = (value << 8) | u64::from(*byte);
    }
    let width = len * 7;
    let unknown = width < 64 && value == (1u64 << width) - 1;
    Some((value, len, unknown))
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

    fn png_crc(data: &[u8]) -> u32 {
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

    fn png_with_orientation(image: &RgbImage, orientation: u8, prefix: bool) -> Vec<u8> {
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
            .unwrap();
        let tiff = tiff_orientation(orientation, false);
        let mut payload = Vec::new();
        if prefix {
            payload.extend_from_slice(b"Exif\0\0");
        }
        payload.extend_from_slice(&tiff);
        let mut typed = b"eXIf".to_vec();
        typed.extend_from_slice(&payload);
        let crc = png_crc(&typed);
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        chunk.extend_from_slice(&typed);
        chunk.extend_from_slice(&crc.to_be_bytes());
        let mut index = 8usize;
        while index + 8 <= encoded.len() {
            let len = u32::from_be_bytes(encoded[index..index + 4].try_into().unwrap()) as usize;
            if &encoded[index + 4..index + 8] == b"IDAT" {
                let mut out = Vec::with_capacity(encoded.len() + chunk.len());
                out.extend_from_slice(&encoded[..index]);
                out.extend_from_slice(&chunk);
                out.extend_from_slice(&encoded[index..]);
                return out;
            }
            index += 12 + len;
        }
        panic!("png had no image data");
    }

    #[test]
    fn png_orientation_six_matches_a_clockwise_quarter_turn() {
        let shown = RgbImage::from_raw(3, 2, vec![10, 0, 0, 20, 0, 0, 30, 0, 0, 40, 0, 0, 50, 0, 0, 60, 0, 0]).unwrap();
        let stored = image::imageops::rotate270(&shown);
        for prefix in [false, true] {
            let path = write_bytes("png6", &png_with_orientation(&stored, 6, prefix));
            let opened = open_oriented(&path).unwrap();
            let _ = fs::remove_file(&path);
            assert_eq!(opened.dimensions(), shown.dimensions(), "prefix {prefix}");
            assert_eq!(opened.as_raw(), shown.as_raw(), "prefix {prefix}");
        }
    }

    #[test]
    fn a_png_without_an_orientation_tag_keeps_the_stored_pixels() {
        let stored = RgbImage::from_raw(2, 2, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]).unwrap();
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(stored.as_raw(), stored.width(), stored.height(), image::ExtendedColorType::Rgb8)
            .unwrap();
        let path = write_bytes("png1", &encoded);
        let opened = open_oriented(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(opened.dimensions(), stored.dimensions());
        assert_eq!(opened.as_raw(), stored.as_raw());
    }

    fn tiff_entry(tag: u16, kind: u16, count: u32, value: u32) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[0..2].copy_from_slice(&tag.to_le_bytes());
        out[2..4].copy_from_slice(&kind.to_le_bytes());
        out[4..8].copy_from_slice(&count.to_le_bytes());
        out[8..12].copy_from_slice(&value.to_le_bytes());
        out
    }

    /// Uncompressed pages. `samples` is 1, 3, or 4. `orientation` is the camera tag for that page.
    fn coded_tiff(pages: &[(u32, u32, u16, u16, u16, &[u8])]) -> Vec<u8> {
        let entry_count = 10u16;
        let ifd_len = 2 + usize::from(entry_count) * 12 + 4;
        let mut cursor = 8usize;
        let mut layout = Vec::new();
        for (width, height, _photometric, samples, _orientation, bytes) in pages {
            assert_eq!(bytes.len(), (*width as usize) * (*height as usize) * (*samples as usize));
            let ifd = cursor;
            let bits = ifd + ifd_len;
            let pixels = bits + (*samples as usize) * 2;
            cursor = pixels + bytes.len();
            layout.push((ifd, bits, pixels));
        }
        let mut out = vec![0u8; cursor];
        out[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
        out[4..8].copy_from_slice(&(layout[0].0 as u32).to_le_bytes());
        for (index, (width, height, photometric, samples, orientation, bytes)) in pages.iter().enumerate() {
            let (ifd, bits, pixels) = layout[index];
            let next = if index + 1 < layout.len() { layout[index + 1].0 as u32 } else { 0 };
            out[ifd..ifd + 2].copy_from_slice(&entry_count.to_le_bytes());
            let bits_value = if *samples == 1 { 8 } else { bits as u32 };
            let entries = [
                tiff_entry(256, 4, 1, *width),
                tiff_entry(257, 4, 1, *height),
                tiff_entry(258, 3, u32::from(*samples), bits_value),
                tiff_entry(259, 3, 1, 1),
                tiff_entry(262, 3, 1, u32::from(*photometric)),
                tiff_entry(273, 4, 1, pixels as u32),
                tiff_entry(274, 3, 1, u32::from(*orientation)),
                tiff_entry(277, 3, 1, u32::from(*samples)),
                tiff_entry(278, 4, 1, *height),
                tiff_entry(279, 4, 1, bytes.len() as u32),
            ];
            let mut at = ifd + 2;
            for entry in entries {
                out[at..at + 12].copy_from_slice(&entry);
                at += 12;
            }
            out[at..at + 4].copy_from_slice(&next.to_le_bytes());
            for sample in 0..*samples {
                let at = bits + usize::from(sample) * 2;
                out[at..at + 2].copy_from_slice(&8u16.to_le_bytes());
            }
            out[pixels..pixels + bytes.len()].copy_from_slice(bytes);
        }
        out
    }

    fn rgb_tiff(pages: &[(u32, u32, &[u8], u16)]) -> Vec<u8> {
        let coded: Vec<_> = pages
            .iter()
            .map(|(width, height, rgb, orientation)| (*width, *height, 2u16, 3u16, *orientation, *rgb))
            .collect();
        coded_tiff(&coded)
    }

    #[test]
    fn a_tiff_orientation_tag_turns_the_stored_pixels() {
        let shown = RgbImage::from_raw(3, 2, vec![10, 0, 0, 20, 0, 0, 30, 0, 0, 40, 0, 0, 50, 0, 0, 60, 0, 0]).unwrap();
        let stored = image::imageops::rotate270(&shown);
        let path = write_bytes("tif6", &rgb_tiff(&[(stored.width(), stored.height(), stored.as_raw(), 6)]));
        let pages = super::read_tiff_pages(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].dimensions(), shown.dimensions());
        assert_eq!(pages[0].as_raw(), shown.as_raw());
    }

    #[test]
    fn a_tiff_without_an_orientation_tag_keeps_the_stored_pixels() {
        let stored = RgbImage::from_raw(2, 2, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]).unwrap();
        let path = write_bytes("tif1", &rgb_tiff(&[(stored.width(), stored.height(), stored.as_raw(), 1)]));
        let pages = super::read_tiff_pages(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages[0].as_raw(), stored.as_raw());
    }

    #[test]
    fn every_tiff_page_is_read() {
        let first = RgbImage::from_raw(2, 2, vec![1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4]).unwrap();
        let second = RgbImage::from_raw(2, 2, vec![9, 9, 9, 8, 8, 8, 7, 7, 7, 6, 6, 6]).unwrap();
        let path = write_bytes(
            "tifpages",
            &rgb_tiff(&[
                (first.width(), first.height(), first.as_raw(), 1),
                (second.width(), second.height(), second.as_raw(), 1),
            ]),
        );
        let pages = super::read_tiff_pages(&path).unwrap();
        let info = super::probe(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].as_raw(), first.as_raw());
        assert_eq!(pages[1].as_raw(), second.as_raw());
        assert!(info.video);
        assert!(info.frames_exact);
        assert_eq!(info.frames, 2);
        assert_eq!(info.fps, 1.0);
        assert_eq!(info.duration_sec, 2.0);
    }

    #[test]
    fn a_gray_tiff_and_an_rgba_tiff_become_rgb() {
        let gray = coded_tiff(&[(2, 1, 1, 1, 1, &[9, 4])]);
        let path = write_bytes("tifgray", &gray);
        let pages = super::read_tiff_pages(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages[0].as_raw(), &[9, 9, 9, 4, 4, 4]);
        let rgba = coded_tiff(&[(1, 1, 2, 4, 1, &[1, 2, 3, 255])]);
        let path = write_bytes("tifrgba", &rgba);
        let pages = super::read_tiff_pages(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages[0].as_raw(), &[1, 2, 3]);
    }

    /// 16-bit samples, little-endian, each 8-bit value stored in the high byte.
    fn wide_tiff(pages: &[(u32, u32, u16, u16, u16, &[u8])]) -> Vec<u8> {
        let widened: Vec<(u32, u32, u16, u16, u16, Vec<u8>)> = pages
            .iter()
            .map(|(width, height, photometric, samples, orientation, bytes)| {
                assert_eq!(bytes.len(), (*width as usize) * (*height as usize) * (*samples as usize));
                let mut wide = Vec::with_capacity(bytes.len() * 2);
                for byte in *bytes {
                    // The 8-bit sample sits in the high byte.
                    wide.extend_from_slice(&(u16::from(*byte) << 8).to_le_bytes());
                }
                (*width, *height, *photometric, *samples, *orientation, wide)
            })
            .collect();
        let entry_count = 10u16;
        let ifd_len = 2 + usize::from(entry_count) * 12 + 4;
        let mut cursor = 8usize;
        let mut layout = Vec::new();
        for (_, _, _, samples, _, bytes) in &widened {
            let ifd = cursor;
            let bits = ifd + ifd_len;
            let pixels = bits + (*samples as usize) * 2;
            cursor = pixels + bytes.len();
            layout.push((ifd, bits, pixels));
        }
        let mut out = vec![0u8; cursor];
        out[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
        out[4..8].copy_from_slice(&(layout[0].0 as u32).to_le_bytes());
        for (index, (width, height, photometric, samples, orientation, bytes)) in widened.iter().enumerate() {
            let (ifd, bits, pixels) = layout[index];
            let next = if index + 1 < layout.len() { layout[index + 1].0 as u32 } else { 0 };
            out[ifd..ifd + 2].copy_from_slice(&entry_count.to_le_bytes());
            let bits_value = if *samples == 1 { 16 } else { bits as u32 };
            let entries = [
                tiff_entry(256, 4, 1, *width),
                tiff_entry(257, 4, 1, *height),
                tiff_entry(258, 3, u32::from(*samples), bits_value),
                tiff_entry(259, 3, 1, 1),
                tiff_entry(262, 3, 1, u32::from(*photometric)),
                tiff_entry(273, 4, 1, pixels as u32),
                tiff_entry(274, 3, 1, u32::from(*orientation)),
                tiff_entry(277, 3, 1, u32::from(*samples)),
                tiff_entry(278, 4, 1, *height),
                tiff_entry(279, 4, 1, bytes.len() as u32),
            ];
            let mut at = ifd + 2;
            for entry in entries {
                out[at..at + 12].copy_from_slice(&entry);
                at += 12;
            }
            out[at..at + 4].copy_from_slice(&next.to_le_bytes());
            for sample in 0..*samples {
                let at = bits + usize::from(sample) * 2;
                out[at..at + 2].copy_from_slice(&16u16.to_le_bytes());
            }
            out[pixels..pixels + bytes.len()].copy_from_slice(bytes);
        }
        out
    }

    #[test]
    fn a_sixteen_bit_tiff_keeps_the_high_byte_and_the_orientation_tag() {
        let shown = RgbImage::from_raw(3, 2, vec![10, 0, 0, 20, 0, 0, 30, 0, 0, 40, 0, 0, 50, 0, 0, 60, 0, 0]).unwrap();
        let stored = image::imageops::rotate270(&shown);
        let path = write_bytes(
            "tif16",
            &wide_tiff(&[(stored.width(), stored.height(), 2, 3, 6, stored.as_raw())]),
        );
        let pages = super::read_tiff_pages(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages[0].dimensions(), shown.dimensions());
        assert_eq!(pages[0].as_raw(), shown.as_raw());
        let gray = wide_tiff(&[(2, 1, 1, 1, 1, &[9, 4])]);
        let path = write_bytes("tif16gray", &gray);
        let pages = super::read_tiff_pages(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages[0].as_raw(), &[9, 9, 9, 4, 4, 4]);
        let rgba = wide_tiff(&[(1, 1, 2, 4, 1, &[1, 2, 3, 255])]);
        let path = write_bytes("tif16rgba", &rgba);
        let pages = super::read_tiff_pages(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages[0].as_raw(), &[1, 2, 3]);
    }

    #[test]
    fn a_truncated_tiff_is_refused() {
        let path = write_bytes("tifbad", &[0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]);
        assert!(super::probe(&path).is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn cmyk_and_studio_ycbcr_become_rgb() {
        let cmyk = coded_tiff(&[(
            3,
            1,
            5,
            4,
            1,
            &[0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0],
        )]);
        let path = write_bytes("tifcmyk", &cmyk);
        let pages = super::read_tiff_pages(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages[0].as_raw(), &[255, 255, 255, 0, 0, 0, 0, 255, 255]);
        let ycbcr = coded_tiff(&[(2, 1, 6, 3, 1, &[235, 128, 128, 16, 128, 128])]);
        let path = write_bytes("tifycc", &ycbcr);
        let pages = super::read_tiff_pages(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(pages[0].as_raw(), &[255, 255, 255, 0, 0, 0]);
    }

    /// Palette page. Index 0 is black and index 1 is white.
    fn indexed_tiff(width: u32, height: u32, indices: &[u8], orientation: u16) -> Vec<u8> {
        assert_eq!(indices.len(), width as usize * height as usize);
        let entry_count = 11u16;
        let ifd = 8usize;
        let ifd_len = 2 + usize::from(entry_count) * 12 + 4;
        let map_at = ifd + ifd_len;
        let pixels = map_at + 256 * 3 * 2;
        let mut out = vec![0u8; pixels + indices.len()];
        out[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
        out[4..8].copy_from_slice(&(ifd as u32).to_le_bytes());
        out[ifd..ifd + 2].copy_from_slice(&entry_count.to_le_bytes());
        let entries = [
            tiff_entry(256, 4, 1, width),
            tiff_entry(257, 4, 1, height),
            tiff_entry(258, 3, 1, 8),
            tiff_entry(259, 3, 1, 1),
            tiff_entry(262, 3, 1, 3),
            tiff_entry(273, 4, 1, pixels as u32),
            tiff_entry(274, 3, 1, u32::from(orientation)),
            tiff_entry(277, 3, 1, 1),
            tiff_entry(278, 4, 1, height),
            tiff_entry(279, 4, 1, indices.len() as u32),
            tiff_entry(320, 3, 256 * 3, map_at as u32),
        ];
        let mut at = ifd + 2;
        for entry in entries {
            out[at..at + 12].copy_from_slice(&entry);
            at += 12;
        }
        for plane in 0..3 {
            let slot = map_at + plane * 256 * 2 + 2;
            out[slot..slot + 2].copy_from_slice(&65535u16.to_le_bytes());
        }
        out[pixels..pixels + indices.len()].copy_from_slice(indices);
        out
    }

    fn bilevel_tiff(width: u32, packed: &[u8], orientation: u16) -> Vec<u8> {
        let height = 1u32;
        let entry_count = 10u16;
        let ifd = 8usize;
        let ifd_len = 2 + usize::from(entry_count) * 12 + 4;
        let pixels = ifd + ifd_len;
        let mut out = vec![0u8; pixels + packed.len()];
        out[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
        out[4..8].copy_from_slice(&(ifd as u32).to_le_bytes());
        out[ifd..ifd + 2].copy_from_slice(&entry_count.to_le_bytes());
        let entries = [
            tiff_entry(256, 4, 1, width),
            tiff_entry(257, 4, 1, height),
            tiff_entry(258, 3, 1, 1),
            tiff_entry(259, 3, 1, 1),
            tiff_entry(262, 3, 1, 1),
            tiff_entry(273, 4, 1, pixels as u32),
            tiff_entry(274, 3, 1, u32::from(orientation)),
            tiff_entry(277, 3, 1, 1),
            tiff_entry(278, 4, 1, height),
            tiff_entry(279, 4, 1, packed.len() as u32),
        ];
        let mut at = ifd + 2;
        for entry in entries {
            out[at..at + 12].copy_from_slice(&entry);
            at += 12;
        }
        out[pixels..pixels + packed.len()].copy_from_slice(packed);
        out
    }

    #[test]
    fn a_palette_and_a_bilevel_tiff_are_shown_with_the_orientation_tag() {
        let shown = RgbImage::from_raw(2, 1, vec![0, 0, 0, 255, 255, 255]).unwrap();
        let stored = image::imageops::rotate270(&shown);
        let indices: Vec<u8> = stored.pixels().map(|pixel| if pixel[0] == 0 { 0 } else { 1 }).collect();
        let path = write_bytes("tifpal", &indexed_tiff(stored.width(), stored.height(), &indices, 6));
        let mut frames = Vec::new();
        let stats = super::for_each_frame(&path, |_, frame| {
            frames.push(frame.clone());
            true
        })
        .unwrap();
        let info = super::probe(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(stats.frames_decoded, 1);
        assert!(!info.video);
        assert_eq!(info.frames, 1);
        assert_eq!(frames[0].dimensions(), shown.dimensions());
        assert_eq!(frames[0].as_raw(), shown.as_raw());
        let path = write_bytes("tifbit", &bilevel_tiff(8, &[0xF0], 1));
        let mut frames = Vec::new();
        super::for_each_frame(&path, |_, frame| {
            frames.push(frame.clone());
            true
        })
        .unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(frames[0].dimensions(), (8, 1));
        let expected = [255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(frames[0].as_raw(), &expected);
    }

    fn indexed_pages(pages: &[(u32, u32, &[u8], u16)]) -> Vec<u8> {
        let entry_count = 11usize;
        let ifd_len = 2 + entry_count * 12 + 4;
        let map_len = 256 * 3 * 2;
        let mut cursor = 8usize;
        let mut layout = Vec::new();
        for (_, _, indices, _) in pages {
            let ifd = cursor;
            let map_at = ifd + ifd_len;
            let pixels = map_at + map_len;
            cursor = pixels + indices.len();
            layout.push((ifd, map_at, pixels));
        }
        let mut out = vec![0u8; cursor];
        out[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
        out[4..8].copy_from_slice(&(layout[0].0 as u32).to_le_bytes());
        for (index, (width, height, indices, orientation)) in pages.iter().enumerate() {
            let (ifd, map_at, pixels) = layout[index];
            let next = if index + 1 < layout.len() { layout[index + 1].0 as u32 } else { 0 };
            out[ifd..ifd + 2].copy_from_slice(&11u16.to_le_bytes());
            let entries = [
                tiff_entry(256, 4, 1, *width),
                tiff_entry(257, 4, 1, *height),
                tiff_entry(258, 3, 1, 8),
                tiff_entry(259, 3, 1, 1),
                tiff_entry(262, 3, 1, 3),
                tiff_entry(273, 4, 1, pixels as u32),
                tiff_entry(274, 3, 1, u32::from(*orientation)),
                tiff_entry(277, 3, 1, 1),
                tiff_entry(278, 4, 1, *height),
                tiff_entry(279, 4, 1, indices.len() as u32),
                tiff_entry(320, 3, 256 * 3, map_at as u32),
            ];
            let mut at = ifd + 2;
            for entry in entries {
                out[at..at + 12].copy_from_slice(&entry);
                at += 12;
            }
            out[at..at + 4].copy_from_slice(&next.to_le_bytes());
            for plane in 0..3 {
                let slot = map_at + plane * 256 * 2 + 2;
                out[slot..slot + 2].copy_from_slice(&65535u16.to_le_bytes());
            }
            out[pixels..pixels + indices.len()].copy_from_slice(indices);
        }
        out
    }

    #[test]
    fn a_multipage_palette_tiff_is_refused() {
        let bytes = indexed_pages(&[(2, 2, &[0, 0, 0, 0], 1), (2, 2, &[1, 1, 1, 1], 1)]);
        let path = write_bytes("tifpal2", &bytes);
        assert!(super::for_each_frame(&path, |_, _| true).is_err());
        assert!(super::probe(&path).is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_tiff_decode_stops_when_the_scan_stops() {
        let page = [1u8, 2, 3];
        let path = write_bytes("tifstop", &rgb_tiff(&[(1, 1, &page, 1), (1, 1, &page, 1)]));
        let stats = super::for_each_frame(&path, |_, _| false).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(stats.frames_decoded, 1);
        assert!(!stats.clean);
    }

    fn webp_lossless(image: &RgbImage) -> Vec<u8> {
        let mut encoded = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
            .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
            .unwrap();
        encoded
    }

    fn tiff_orientation(tag: u8, big_endian: bool) -> Vec<u8> {
        let mut tiff = Vec::new();
        if big_endian {
            tiff.extend_from_slice(&[0x4D, 0x4D, 0x00, 0x2A]);
            tiff.extend_from_slice(&8u32.to_be_bytes());
            tiff.extend_from_slice(&1u16.to_be_bytes());
            tiff.extend_from_slice(&0x0112u16.to_be_bytes());
            tiff.extend_from_slice(&3u16.to_be_bytes());
            tiff.extend_from_slice(&1u32.to_be_bytes());
            tiff.extend_from_slice(&u16::from(tag).to_be_bytes());
            tiff.extend_from_slice(&0u16.to_be_bytes());
        } else {
            tiff.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
            tiff.extend_from_slice(&8u32.to_le_bytes());
            tiff.extend_from_slice(&1u16.to_le_bytes());
            tiff.extend_from_slice(&0x0112u16.to_le_bytes());
            tiff.extend_from_slice(&3u16.to_le_bytes());
            tiff.extend_from_slice(&1u32.to_le_bytes());
            tiff.extend_from_slice(&u16::from(tag).to_le_bytes());
            tiff.extend_from_slice(&0u16.to_le_bytes());
            tiff.extend_from_slice(&0u32.to_le_bytes());
        }
        tiff
    }

    /// A still WebP with an EXIF chunk. `prefix` writes the JPEG marker in front of the TIFF header.
    fn webp_with_orientation(image: &RgbImage, orientation: u8, big_endian: bool, prefix: bool) -> Vec<u8> {
        let encoded = webp_lossless(image);
        let mut chunks = Vec::new();
        let mut index = 12usize;
        while index + 8 <= encoded.len() {
            let tag = encoded[index..index + 4].to_vec();
            let size = u32::from_le_bytes(encoded[index + 4..index + 8].try_into().unwrap()) as usize;
            let start = index + 8;
            let end = start + size;
            chunks.push((tag, encoded[start..end].to_vec()));
            index = end + (size & 1);
        }
        let mut exif = Vec::new();
        if prefix {
            exif.extend_from_slice(b"Exif\0\0");
        }
        exif.extend_from_slice(&tiff_orientation(orientation, big_endian));
        let mut extended = false;
        for (tag, payload) in chunks.iter_mut() {
            if tag == b"VP8X" && !payload.is_empty() {
                payload[0] |= 0x08;
                extended = true;
            }
        }
        chunks.retain(|(tag, _)| tag.as_slice() != b"EXIF");
        if !extended {
            let width = image.width() - 1;
            let height = image.height() - 1;
            let payload = vec![
                0x08,
                0,
                0,
                0,
                (width & 0xff) as u8,
                ((width >> 8) & 0xff) as u8,
                ((width >> 16) & 0xff) as u8,
                (height & 0xff) as u8,
                ((height >> 8) & 0xff) as u8,
                ((height >> 16) & 0xff) as u8,
            ];
            chunks.insert(0, (b"VP8X".to_vec(), payload));
        }
        chunks.push((b"EXIF".to_vec(), exif));
        let mut body = Vec::new();
        body.extend_from_slice(b"WEBP");
        for (tag, payload) in chunks {
            body.extend_from_slice(&tag);
            body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            body.extend_from_slice(&payload);
            if payload.len() % 2 == 1 {
                body.push(0);
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    fn write_bytes(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("ow-{name}-{}", std::process::id()));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn webp_orientation_six_matches_a_clockwise_quarter_turn() {
        let shown = RgbImage::from_raw(3, 2, vec![10, 0, 0, 20, 0, 0, 30, 0, 0, 40, 0, 0, 50, 0, 0, 60, 0, 0]).unwrap();
        let stored = image::imageops::rotate270(&shown);
        for (big, prefix) in [(true, false), (false, true)] {
            let path = write_bytes("webp6", &webp_with_orientation(&stored, 6, big, prefix));
            let opened = open_oriented(&path).unwrap();
            let _ = fs::remove_file(&path);
            assert_eq!(opened.dimensions(), shown.dimensions(), "endian {big} prefix {prefix}");
            assert_eq!(opened.as_raw(), shown.as_raw(), "endian {big} prefix {prefix}");
        }
    }

    #[test]
    fn a_webp_without_an_orientation_tag_keeps_the_stored_pixels() {
        let stored = RgbImage::from_raw(2, 2, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]).unwrap();
        let path = write_bytes("webp1", &webp_lossless(&stored));
        let opened = open_oriented(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(opened.dimensions(), stored.dimensions());
        assert_eq!(opened.as_raw(), stored.as_raw());
    }

    #[test]
    fn an_animated_webp_is_not_a_still() {
        let stored = RgbImage::from_raw(2, 2, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]).unwrap();
        let mut bytes = webp_lossless(&stored);
        let anim = b"ANIM";
        bytes.extend_from_slice(anim);
        bytes.extend_from_slice(&6u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        let path = write_bytes("webpanim", &bytes);
        let probed = super::probe(&path);
        let _ = fs::remove_file(&path);
        match probed {
            Ok(info) => assert!(info.video, "an animated webp was read as one still"),
            Err(super::MediaError::BadCodec) => {}
        }
        assert!(super::animated_webp(&write_bytes("webpanim2", &bytes)));
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

    #[test]
    fn non_square_pixels_widen_to_the_shown_frame() {
        assert_eq!(super::sample_aspect(None), (1, 1));
        assert_eq!(super::sample_aspect(Some("N/A")), (1, 1));
        assert_eq!(super::sample_aspect(Some("1:1")), (1, 1));
        assert_eq!(super::sample_aspect(Some("2:1")), (2, 1));
        assert_eq!(super::square_width(320, 2, 1), 640);
        assert_eq!(super::square_width(320, 3, 2), 480);
        assert_eq!(super::displayed_size(480, 320, 1, 2, true), (640, 480, true));
        assert_eq!(super::displayed_size(320, 480, 2, 1, true), (240, 320, true));
        assert_eq!(super::displayed_size(320, 480, 2, 1, false), (640, 480, true));
        assert_eq!(super::displayed_size(480, 640, 1, 1, true), (640, 480, false));
    }

    #[test]
    fn a_track_clock_is_the_picture_length() {
        assert_eq!(super::parse_clock("00:00:00.800000000"), Some(0.8));
        assert_eq!(super::parse_clock("00:01:02.5"), Some(62.5));
        assert_eq!(super::parse_clock("nope"), None);
        let matroska = serde_json::json!({"duration": "30.023", "tags": {"DURATION": "00:00:00.800000000"}});
        assert!((super::picture_duration(&matroska, 30.023) - 0.8).abs() < 0.001);
        let mp4 = serde_json::json!({"duration": "0.800000", "tags": {}});
        assert!((super::picture_duration(&mp4, 30.0) - 0.8).abs() < 0.001);
        let bare = serde_json::json!({});
        assert!((super::picture_duration(&bare, 4.0) - 4.0).abs() < 0.001);
    }

    #[test]
    fn a_movie_header_clock_is_seconds_since_1904() {
        let dir = tempfile::tempdir().unwrap();
        let mp4 = dir.path().join("clock.mp4");
        fs::write(&mp4, movie_clock(3_660_681_600)).unwrap();
        let when = crate::timeutil::parse_rfc3339("2020-01-01T00:00:00Z").unwrap();
        assert_eq!(super::container_created(&mp4), Some(when));
        fs::write(&mp4, movie_clock(0)).unwrap();
        assert!(super::container_created(&mp4).is_none());
        let mkv = dir.path().join("clock.mkv");
        fs::write(&mkv, matroska_clock(599_529_600_000_000_000)).unwrap();
        assert_eq!(super::container_created(&mkv), Some(when));
        fs::write(dir.path().join("still.png"), []).unwrap();
        assert!(super::container_created(&dir.path().join("still.png")).is_none());
    }

    fn movie_clock(created: u32) -> Vec<u8> {
        fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = ((8 + body.len()) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out
        }
        let mut header = vec![0u8; 100];
        header[4..8].copy_from_slice(&created.to_be_bytes());
        let mut file = atom(b"ftyp", b"isom");
        file.extend(atom(b"moov", &atom(b"mvhd", &header)));
        file
    }

    fn matroska_clock(ns: i64) -> Vec<u8> {
        fn element(id: &[u8], body: &[u8]) -> Vec<u8> {
            let mut out = id.to_vec();
            assert!(body.len() < 127);
            out.push(0x80 | body.len() as u8);
            out.extend_from_slice(body);
            out
        }
        let date = element(&[0x44, 0x61], &ns.to_be_bytes());
        let info = element(&[0x15, 0x49, 0xA9, 0x66], &date);
        let segment = element(&[0x18, 0x53, 0x80, 0x67], &info);
        let mut file = element(&[0x1A, 0x45, 0xDF, 0xA3], &[]);
        file.extend(segment);
        file
    }
}
