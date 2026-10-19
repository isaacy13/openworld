// SPDX-License-Identifier: Apache-2.0

//! Desktop file scans use FFmpeg for video and the image crate for stills.
//! A JPEG, a still WebP, a PNG, or a TIFF is turned to match its camera orientation
//! tag before the pixels are scanned. An animated PNG is turned the same way, on
//! every frame. An animated WebP is every frame of that file. Every page of a TIFF is scanned. A 16-bit
//! sample contributes its high 8 bits. CMYK and full-resolution YCbCr become RGB.
//! A one-page palette, bilevel, or subsampled TIFF is read as a player shows that
//! page. A multi-page TIFF whose pages are not all read is refused. Audio is not decoded. Phone, Mac, and Android shells
//! decode with AVFoundation or MediaCodec and push frames to the engine one at a
//! time. That path does not run FFmpeg. It reads the container creation time from
//! the file, so a clock that disagrees with the file time warns the same way the
//! desktop probe does.
//!
//! Untrusted files are read in bounded memory: one frame at a time, headers by
//! seeking rather than reading the file, and a picture over `MAX_PIXELS` is
//! refused before it is decoded. FFmpeg reads only local files.

use crate::timeutil::parse_rfc3339;
use image::metadata::Orientation;
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageReader, RgbImage};
use serde_json::Value;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("bad codec or unreadable file")]
    BadCodec,
    #[error("picture too large")]
    TooLarge,
}

/// A picture with more pixels than this is refused before it is decoded: 120 megapixels.
pub const MAX_PIXELS: u64 = 120_000_000;
/// Pages of a TIFF, or frames of an animated WebP, read before refusing.
const MAX_STILL_FRAMES: u64 = 10_000;
/// A container header read into memory is never larger than this.
const MAX_HEADER_BYTES: u64 = 4 * 1024 * 1024;
/// FFmpeg or ffprobe that writes nothing for this long is stopped: a crafted
/// file must not hang a scan that no Stop can reach while the read blocks.
const STALL: Duration = Duration::from_secs(120);

/// Kills `child` when `limit` passes without a beat on the returned sender.
/// Dropping the sender ends the watch. The handle says whether it killed.
fn watch(child: &Arc<Mutex<Child>>, limit: Duration) -> (mpsc::Sender<()>, JoinHandle<bool>) {
    let (beat, beats) = mpsc::channel();
    let child = Arc::clone(child);
    let dog = std::thread::spawn(move || loop {
        match beats.recv_timeout(limit) {
            Ok(()) => {}
            Err(RecvTimeoutError::Disconnected) => return false,
            Err(RecvTimeoutError::Timeout) => {
                if let Ok(mut child) = child.lock() {
                    let _ = child.kill();
                }
                return true;
            }
        }
    });
    (beat, dog)
}

trait WatchedOutput {
    /// `Command::output`, stopped when the program goes `STALL` without writing.
    fn watched_output(&mut self) -> std::io::Result<Output>;
}

impl WatchedOutput for Command {
    fn watched_output(&mut self) -> std::io::Result<Output> {
        let mut child = self.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            return Err(std::io::Error::other("no pipes"));
        };
        let errors = std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = stderr.read_to_end(&mut text);
            text
        });
        let child = Arc::new(Mutex::new(child));
        let (beat, dog) = watch(&child, STALL);
        let mut out = Vec::new();
        let mut chunk = [0u8; 64 * 1024];
        while let Ok(n @ 1..) = stdout.read(&mut chunk) {
            out.extend_from_slice(&chunk[..n]);
            let _ = beat.send(());
        }
        drop(beat);
        let stalled = dog.join().unwrap_or(true);
        let status = child.lock().map_err(|_| std::io::Error::other("lock"))?.wait()?;
        let stderr = errors.join().unwrap_or_default();
        if stalled {
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "stalled"));
        }
        Ok(Output { status, stdout: out, stderr })
    }
}

fn fits(width: u32, height: u32) -> Result<(), MediaError> {
    if width == 0 || height == 0 {
        return Err(MediaError::BadCodec);
    }
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(MediaError::TooLarge);
    }
    Ok(())
}

/// A path FFmpeg reads as a local file, even when its name starts with `-` or has a `:`.
fn ffmpeg_input(path: &Path) -> std::ffi::OsString {
    let mut arg = std::ffi::OsString::from("file:");
    arg.push(path.as_os_str());
    arg
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
    /// Which video stream a player shows, as `v:N`. Stills leave this at 0.
    pub shown_stream: usize,
    /// The last frame is the JPEG cover of a motion photo. Measured coverage reads it.
    pub motion_cover: bool,
    /// Every frame is its own picture, as the pages of a TIFF are.
    pub pages: bool,
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
        return match probe_tiff(path) {
            Err(MediaError::BadCodec) => probe_tiff_player(path),
            other => other,
        };
    }
    // A motion photo is a still plus a video. The scan reads both. The
    // cover is one more picture after the clip, so the clip's frame numbers stay.
    if let Some(clip) = motion_clip(path)? {
        let mut info = probe_video(&clip.path)?;
        info.frames = info.frames.saturating_add(1);
        info.motion_cover = true;
        return Ok(info);
    }
    if animated_webp(path) {
        return probe_animated_webp(path);
    }
    if let Some(info) = probe_image(path) {
        fits(info.width, info.height)?;
        return Ok(info);
    }
    probe_video(path)
}

pub fn for_each_frame(path: &Path, mut on_frame: impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    if is_tiff(path) {
        return match probe_tiff(path) {
            Ok(info) => decode_tiff(path, info, &mut on_frame),
            Err(MediaError::BadCodec) => decode_tiff_player(path, &mut on_frame),
            Err(err) => Err(err),
        };
    }
    if let Some(clip) = motion_clip(path)? {
        let info = probe_video(&clip.path)?;
        let mut stats = decode_video(&clip.path, &info, true, &mut on_frame)?;
        // The cover is a picture in the file. A clip that does not show the face
        // must not clear it. An aborted clip is not followed by another frame.
        if stats.clean {
            // JPEG covers use the image crate. An AVIF or HEIF cover is a container
            // the image crate does not read, so FFmpeg decodes that one picture.
            let cover = open_oriented(path).ok().or_else(|| open_container_cover(path).ok());
            if let Some(cover) = cover {
                let keep = on_frame(stats.frames_decoded, &cover);
                stats.frames_decoded = stats.frames_decoded.saturating_add(1);
                stats.probe.frames = stats.probe.frames.saturating_add(1);
                stats.probe.motion_cover = true;
                if !keep || jpeg_cut_short(path) {
                    stats.clean = false;
                }
            } else {
                // The cover was never read, so the file was not fully scanned.
                stats.clean = false;
            }
        }
        return Ok(stats);
    }
    // FFmpeg's WebP decoder skips an animation and reports no picture. A player
    // still shows every frame, so the scan reads those frames itself.
    if animated_webp(path) {
        return decode_animated_webp(path, &mut on_frame);
    }
    let info = probe(path)?;
    if !info.video {
        let image = open_oriented(path).map_err(|err| match err {
            image::ImageError::Limits(_) => MediaError::TooLarge,
            _ => MediaError::BadCodec,
        })?;
        let keep_going = on_frame(0, &image);
        // The JPEG decoder paints missing data gray and says nothing: a cut file is not clean.
        return Ok(DecodeStats { frames_decoded: 1, clean: keep_going && !jpeg_cut_short(path), probe: info });
    }
    // An animated PNG skips the still path so every frame is read. The camera
    // tag on that file is the same turn a still PNG gets.
    if let Some(orientation) = png_exif_orientation(path) {
        if orientation != Orientation::NoTransforms {
            return decode_video(path, &info, false, &mut |index, frame| {
                let mut image = DynamicImage::ImageRgb8(frame.clone());
                image.apply_orientation(orientation);
                let shown = image.to_rgb8();
                on_frame(index, &shown)
            });
        }
    }
    decode_video(path, &info, true, &mut on_frame)
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

enum MotionVideo {
    None,
    /// The XMP names a motion photo, and the video length cannot be read.
    Broken,
    Span { start: u64, end: u64 },
}

/// The clip copied out of a motion photo. Private to this user, removed on drop.
struct MotionClip {
    path: PathBuf,
    _file: tempfile::TempPath,
}

/// A JPEG whose XMP points at a video appended to the file. The video is copied
/// out so FFmpeg can seek in it. The copy is removed when this value drops.
fn motion_clip(path: &Path) -> Result<Option<MotionClip>, MediaError> {
    let span = match motion_video(path) {
        MotionVideo::None => return Ok(None),
        MotionVideo::Broken => return Err(MediaError::BadCodec),
        MotionVideo::Span { start, end } => (start, end),
    };
    // A random name, created exclusively with owner-only permissions.
    let temp = tempfile::Builder::new().prefix("openworld-motion-").suffix(".mp4").tempfile().map_err(|_| MediaError::BadCodec)?;
    let (mut out, temp) = temp.into_parts();
    let dest = temp.to_path_buf();
    let mut src = File::open(path).map_err(|_| MediaError::BadCodec)?;
    src.seek(SeekFrom::Start(span.0)).map_err(|_| MediaError::BadCodec)?;
    let mut left = span.1 - span.0;
    let mut buf = [0u8; 65536];
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        let read = src.read(&mut buf[..want]).map_err(|_| MediaError::BadCodec)?;
        if read == 0 {
            return Err(MediaError::BadCodec);
        }
        out.write_all(&buf[..read]).map_err(|_| MediaError::BadCodec)?;
        left -= read as u64;
    }
    Ok(Some(MotionClip { path: dest, _file: temp }))
}

/// `MicroVideoOffset` or a MotionPhoto item length is the video at the end of the file.
/// A JPEG stores that length in an XMP marker. An AVIF or HEIF stores it in a `uuid` box,
/// and the clip itself is an `mpvd` box. A Samsung trailer holds the clip before `SEFT` instead.
fn motion_video(path: &Path) -> MotionVideo {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return MotionVideo::None,
    };
    let len = match file.metadata() {
        Ok(meta) => meta.len(),
        Err(_) => return MotionVideo::None,
    };
    let mut soi = [0u8; 2];
    if file.read_exact(&mut soi).is_err() {
        return MotionVideo::None;
    }
    if soi != [0xFF, 0xD8] {
        if file.seek(SeekFrom::Start(0)).is_err() {
            return MotionVideo::None;
        }
        return container_motion(&mut file, len);
    }
    let mut claimed = false;
    let mut turned_off = false;
    let mut video_len = None;
    while let Some(marker) = next_jpeg_marker(&mut file) {
        // The scan starts here. The XMP packet is a marker before it.
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        if marker == 0x00 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        let mut lenb = [0u8; 2];
        if file.read_exact(&mut lenb).is_err() {
            break;
        }
        let seglen = u16::from_be_bytes(lenb) as u64;
        if seglen < 2 {
            return MotionVideo::Broken;
        }
        let payload = seglen - 2;
        if marker == 0xE1 && payload <= 1_000_000 {
            let mut buf = vec![0u8; payload as usize];
            if file.read_exact(&mut buf).is_err() {
                break;
            }
            match motion_length_in_xmp(&buf) {
                MotionXmp::No => {}
                MotionXmp::Off => turned_off = true,
                MotionXmp::Video(n) => {
                    claimed = true;
                    if video_len.is_none() {
                        video_len = Some(n);
                    }
                }
                MotionXmp::Unreadable => claimed = true,
            }
        } else if file.seek(SeekFrom::Current(payload as i64)).is_err() {
            break;
        }
    }
    // A value other than 1 is a still, even when a trailer still holds a clip.
    if turned_off {
        return MotionVideo::None;
    }
    // Google's directory and the older offset store the clip as the last bytes.
    if let Some(start) = video_len.and_then(|video_len| video_at_end(&mut file, len, video_len)) {
        return MotionVideo::Span { start, end: len };
    }
    // Samsung stores that clip before an SEFT trailer, so it is not the last bytes.
    if let Some((start, end)) = samsung_video(&mut file, len) {
        return MotionVideo::Span { start, end };
    }
    if !claimed {
        return MotionVideo::None;
    }
    // The packet names a motion photo. A missing length is not a still.
    // A length that does not land on a movie means the video was removed.
    if video_len.is_none() {
        return MotionVideo::Broken;
    }
    MotionVideo::None
}

/// The still in an AVIF or HEIF motion photo. The image crate does not read those files.
fn open_container_cover(path: &Path) -> Result<RgbImage, MediaError> {
    let mut info = probe_video(path)?;
    info.frames = 1;
    info.frames_exact = true;
    let mut cover = None;
    decode_video(path, &info, true, &mut |_index, frame| {
        cover = Some(frame.clone());
        false
    })?;
    cover.ok_or(MediaError::BadCodec)
}

const XMP_UUID: [u8; 16] = [
    0xBE, 0x7A, 0xCF, 0xCB, 0x97, 0xA9, 0x42, 0xE8, 0x9C, 0x71, 0x99, 0x94, 0x91, 0xE3, 0xAF, 0xAC,
];

struct MotionClaim {
    claimed: bool,
    turned_off: bool,
    video_len: Option<u64>,
}

/// An AVIF or HEIF still whose XMP names a clip. The clip is the last bytes of the file,
/// inside an `mpvd` box. The packet lives in a `uuid` box, not in a JPEG marker.
fn container_motion(file: &mut File, len: u64) -> MotionVideo {
    if !still_image_brand(file) {
        return MotionVideo::None;
    }
    let mut claim = MotionClaim { claimed: false, turned_off: false, video_len: None };
    walk_motion_boxes(file, 0, len, 0, &mut claim);
    if claim.turned_off {
        return MotionVideo::None;
    }
    if let Some(start) = claim.video_len.and_then(|video_len| video_at_end(file, len, video_len)) {
        return MotionVideo::Span { start, end: len };
    }
    if !claim.claimed {
        return MotionVideo::None;
    }
    if claim.video_len.is_none() {
        return MotionVideo::Broken;
    }
    MotionVideo::None
}

fn still_image_brand(file: &mut File) -> bool {
    let mut header = [0u8; 12];
    if file.seek(SeekFrom::Start(0)).is_err() || file.read_exact(&mut header).is_err() {
        return false;
    }
    if &header[4..8] != b"ftyp" {
        return false;
    }
    matches!(
        &header[8..12],
        b"avif" | b"avis" | b"heic" | b"heix" | b"hevc" | b"heif" | b"mif1" | b"msf1"
    )
}

fn note_motion_xmp(claim: &mut MotionClaim, parsed: MotionXmp) {
    match parsed {
        MotionXmp::No => {}
        MotionXmp::Off => claim.turned_off = true,
        MotionXmp::Video(n) => {
            claim.claimed = true;
            if claim.video_len.is_none() {
                claim.video_len = Some(n);
            }
        }
        MotionXmp::Unreadable => claim.claimed = true,
    }
}

/// Top-level boxes, and the children of `meta`. Image bytes in `mdat` are not XMP.
fn walk_motion_boxes(file: &mut File, start: u64, end: u64, depth: u32, claim: &mut MotionClaim) {
    if depth > 6 {
        return;
    }
    let mut pos = start;
    while pos + 8 <= end {
        if file.seek(SeekFrom::Start(pos)).is_err() {
            return;
        }
        let mut hdr = [0u8; 8];
        if file.read_exact(&mut hdr).is_err() {
            return;
        }
        let mut size = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
        let typ = [hdr[4], hdr[5], hdr[6], hdr[7]];
        let mut header = 8u64;
        if size == 1 {
            if pos + 16 > end {
                return;
            }
            let mut ext = [0u8; 8];
            if file.read_exact(&mut ext).is_err() {
                return;
            }
            size = u64::from_be_bytes(ext);
            header = 16;
        }
        let box_end = if size == 0 {
            end
        } else {
            if size < header {
                return;
            }
            match pos.checked_add(size) {
                Some(next) if next <= end => next,
                _ => return,
            }
        };
        if typ == *b"uuid" && box_end >= pos + header + 16 {
            if file.seek(SeekFrom::Start(pos + header)).is_err() {
                return;
            }
            let mut uuid = [0u8; 16];
            if file.read_exact(&mut uuid).is_ok() && uuid == XMP_UUID {
                let payload_at = pos + header + 16;
                let payload_len = (box_end - payload_at).min(1_000_000) as usize;
                if payload_len > 0 {
                    let mut buf = vec![0u8; payload_len];
                    if file.read_exact(&mut buf).is_ok() {
                        note_motion_xmp(claim, motion_length_in_xmp_text(&buf));
                    }
                }
            }
        } else if typ == *b"xml " && box_end > pos + header {
            let payload_at = pos + header;
            let payload_len = (box_end - payload_at).min(1_000_000) as usize;
            if payload_len > 0 && file.seek(SeekFrom::Start(payload_at)).is_ok() {
                let mut buf = vec![0u8; payload_len];
                if file.read_exact(&mut buf).is_ok() {
                    note_motion_xmp(claim, motion_length_in_xmp_text(&buf));
                }
            }
        } else if typ == *b"meta" && depth < 4 && box_end >= pos + header + 4 {
            walk_motion_boxes(file, pos + header + 4, box_end, depth + 1, claim);
        }
        if size == 0 {
            return;
        }
        pos = box_end;
    }
}

/// `video_len` bytes at the end of the file, when those bytes are a movie.
fn video_at_end(file: &mut File, len: u64, video_len: u64) -> Option<u64> {
    if video_len == 0 || video_len >= len {
        return None;
    }
    let start = len - video_len;
    let mut header = [0u8; 8];
    file.seek(SeekFrom::Start(start)).ok()?;
    file.read_exact(&mut header).ok()?;
    if &header[4..8] != b"ftyp" {
        return None;
    }
    Some(start)
}

/// The mp4 embedded in a Samsung trailer. The trailer ends with `SEFT`.
/// The clip starts after the `MotionPhoto_Data` marker, not at the end of the file.
fn samsung_video(file: &mut File, len: u64) -> Option<(u64, u64)> {
    if len < 32 {
        return None;
    }
    let mut tail = [0u8; 8];
    file.seek(SeekFrom::End(-8)).ok()?;
    file.read_exact(&mut tail).ok()?;
    if &tail[4..8] != b"SEFT" {
        return None;
    }
    let sef_size = u64::from(u32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]));
    if sef_size < 12 || sef_size > len - 8 {
        return None;
    }
    let sefh = len - 8 - sef_size;
    let mut head = [0u8; 12];
    file.seek(SeekFrom::Start(sefh)).ok()?;
    file.read_exact(&mut head).ok()?;
    if &head[0..4] != b"SEFH" {
        return None;
    }
    let count = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
    if count == 0 || count > 16 {
        return None;
    }
    let entries_bytes = u64::from(count) * 12;
    if sefh + 12 + entries_bytes > len - 8 {
        return None;
    }
    let mut entries = vec![0u8; entries_bytes as usize];
    file.read_exact(&mut entries).ok()?;
    let marker = b"\x00\x00\x30\x0A";
    for chunk in entries.chunks_exact(12) {
        if &chunk[0..4] != marker {
            continue;
        }
        let back = u64::from(u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]));
        let data_len = u64::from(u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]));
        if back > sefh || data_len < 32 {
            continue;
        }
        let field = sefh - back;
        if field.checked_add(data_len)? > sefh {
            continue;
        }
        let mut hdr = [0u8; 24];
        file.seek(SeekFrom::Start(field)).ok()?;
        file.read_exact(&mut hdr).ok()?;
        if &hdr[0..4] != marker {
            continue;
        }
        let name_len = u64::from(u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]));
        if name_len != 16 || &hdr[8..24] != b"MotionPhoto_Data" {
            continue;
        }
        let start = field + 24;
        let end = field + data_len;
        if start >= end {
            continue;
        }
        let mut box_header = [0u8; 8];
        file.seek(SeekFrom::Start(start)).ok()?;
        file.read_exact(&mut box_header).ok()?;
        if &box_header[4..8] != b"ftyp" {
            continue;
        }
        return Some((start, end));
    }
    None
}

fn next_jpeg_marker(file: &mut File) -> Option<u8> {
    let mut byte = [0u8; 1];
    file.read_exact(&mut byte).ok()?;
    if byte[0] != 0xFF {
        return None;
    }
    loop {
        file.read_exact(&mut byte).ok()?;
        if byte[0] != 0xFF {
            return Some(byte[0]);
        }
    }
}

enum MotionXmp {
    No,
    /// `MotionPhoto` is present and is not 1. The file is a still.
    Off,
    /// The packet names a motion photo and gives the video's byte length.
    Video(u64),
    /// The packet names a motion photo, and the video length is not readable.
    Unreadable,
}

/// `MicroVideoOffset` is the older length. `MotionPhotoOffset` is the same
/// length on a later Pixel file, and it wins when both are present. A directory
/// lists the cover first, with length 0, then the video item. That item's length
/// locates the video, and a leftover offset is ignored. Only `MotionPhoto` 1 is a motion photo.
fn motion_length_in_xmp(buf: &[u8]) -> MotionXmp {
    let prefix = b"http://ns.adobe.com/xap/1.0/\0";
    if !buf.starts_with(prefix) {
        return MotionXmp::No;
    }
    motion_length_in_xmp_text(&buf[prefix.len()..])
}

/// The packet body, without the JPEG APP1 prefix. A `uuid` box stores it that way.
fn motion_length_in_xmp_text(buf: &[u8]) -> MotionXmp {
    let text = String::from_utf8_lossy(buf);
    if motion_photo_off(&text) {
        return MotionXmp::Off;
    }
    if let Some(n) = motion_item_length(&text) {
        return MotionXmp::Video(n);
    }
    if let Some(n) = xmp_attr_u64(&text, "MotionPhotoOffset") {
        return MotionXmp::Video(n);
    }
    if let Some(n) = xmp_attr_u64(&text, "MicroVideoOffset") {
        return MotionXmp::Video(n);
    }
    let named = text.contains("MicroVideoOffset=\"")
        || text.contains("MicroVideoOffset='")
        || text.contains("MotionPhotoOffset=\"")
        || text.contains("MotionPhotoOffset='")
        || text.contains("Semantic=\"MotionPhoto\"")
        || text.contains("Semantic='MotionPhoto'");
    if named {
        MotionXmp::Unreadable
    } else {
        MotionXmp::No
    }
}

/// The length belongs to the MotionPhoto item. The cover item's length, 0, is a different item.
fn motion_item_length(text: &str) -> Option<u64> {
    let idx = text.find("Semantic=\"MotionPhoto\"").or_else(|| text.find("Semantic='MotionPhoto'"))?;
    let start = text[..idx].rfind('<').unwrap_or(0);
    let end_rel = text[idx..].find('>')?;
    xmp_attr_u64(&text[start..idx + end_rel], "Length")
}

/// A present `MotionPhoto` value other than 1 marks the file as a still.
/// `MotionPhotoVersion` and `MotionPhotoPresentationTimestampUs` are different attributes.
fn motion_photo_off(text: &str) -> bool {
    for quote in ['"', '\''] {
        let key = format!("MotionPhoto={quote}");
        let Some(pos) = text.find(&key) else {
            continue;
        };
        let rest = &text[pos + key.len()..];
        let bytes = rest.as_bytes();
        let neg = bytes.first() == Some(&b'-');
        let start = usize::from(neg);
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == start {
            return true;
        }
        return !matches!(rest[start..end].parse::<u64>(), Ok(1) if !neg);
    }
    false
}

fn xmp_attr_u64(text: &str, name: &str) -> Option<u64> {
    for quote in ['"', '\''] {
        let key = format!("{name}={quote}");
        let Some(pos) = text.find(&key) else {
            continue;
        };
        let digits: String = text[pos + key.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<u64>() {
            if n > 0 {
                return Some(n);
            }
        }
    }
    None
}

/// Every page, each turned by that page's orientation tag. Pages are counted
/// from their headers; no pixels are decoded to plan the scan.
fn probe_tiff(path: &Path) -> Result<Probe, MediaError> {
    let file = File::open(path).map_err(|_| MediaError::BadCodec)?;
    let mut decoder = tiff::decoder::Decoder::new(BufReader::new(file)).map_err(|_| MediaError::BadCodec)?;
    let mut count = 0u64;
    let mut first = None;
    loop {
        let (w, h) = decoder.dimensions().map_err(|_| MediaError::BadCodec)?;
        fits(w, h)?;
        tiff_samples(decoder.colortype().map_err(|_| MediaError::BadCodec)?)?;
        if first.is_none() {
            first = Some(oriented_still_size(w, h, tiff_tag(&mut decoder)));
        }
        count += 1;
        if count > MAX_STILL_FRAMES {
            return Err(MediaError::TooLarge);
        }
        if !decoder.more_images() {
            break;
        }
        decoder.next_image().map_err(|_| MediaError::BadCodec)?;
    }
    let (width, height) = first.ok_or(MediaError::BadCodec)?;
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
        shown_stream: 0,
        motion_cover: false,
        pages: moving,
    })
}

/// One page at a time. A page that does not decode refuses the whole file.
fn decode_tiff(path: &Path, info: Probe, on_frame: &mut impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    let file = File::open(path).map_err(|_| MediaError::BadCodec)?;
    let mut decoder = tiff::decoder::Decoder::new(BufReader::new(file)).map_err(|_| MediaError::BadCodec)?;
    let mut decoded = 0u64;
    let mut clean = true;
    loop {
        let page = read_tiff_page(&mut decoder)?;
        let keep = on_frame(decoded, &page);
        decoded += 1;
        if !keep {
            clean = false;
            break;
        }
        if !decoder.more_images() {
            break;
        }
        decoder.next_image().map_err(|_| MediaError::BadCodec)?;
    }
    Ok(DecodeStats { frames_decoded: decoded, clean: clean && decoded == info.frames, probe: info })
}

fn tiff_samples(color: tiff::ColorType) -> Result<(usize, u8), MediaError> {
    match color {
        tiff::ColorType::Gray(bits) if (8..=16).contains(&bits) => Ok((1, bits)),
        tiff::ColorType::RGB(bits) if (8..=16).contains(&bits) => Ok((3, bits)),
        tiff::ColorType::RGBA(bits) if (8..=16).contains(&bits) => Ok((4, bits)),
        tiff::ColorType::CMYK(8) => Ok((4, 8)),
        tiff::ColorType::YCbCr(8) => Ok((3, 8)),
        _ => Err(MediaError::BadCodec),
    }
}

fn tiff_tag<R: Read + Seek>(decoder: &mut tiff::decoder::Decoder<R>) -> u8 {
    decoder
        .find_tag(tiff::tags::Tag::Orientation)
        .ok()
        .flatten()
        .and_then(|value| value.into_u16().ok())
        .map(|value| value as u8)
        .filter(|value| (1..=8).contains(value))
        .unwrap_or(1)
}

fn read_tiff_page<R: Read + Seek>(decoder: &mut tiff::decoder::Decoder<R>) -> Result<RgbImage, MediaError> {
    let (width, height) = decoder.dimensions().map_err(|_| MediaError::BadCodec)?;
    fits(width, height)?;
    let color = decoder.colortype().map_err(|_| MediaError::BadCodec)?;
    let (samples, bits) = tiff_samples(color)?;
    let tag = tiff_tag(decoder);
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

/// The whole file, when it is small enough to hold. A TIFF's directory can be anywhere in it.
fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, MediaError> {
    let len = std::fs::metadata(path).map_err(|_| MediaError::BadCodec)?.len();
    if len > max {
        return Err(MediaError::TooLarge);
    }
    std::fs::read(path).map_err(|_| MediaError::BadCodec)
}

/// One page, shown the way a player shows it, when the page reader cannot expand the samples.
fn probe_tiff_player(path: &Path) -> Result<Probe, MediaError> {
    let bytes = read_bounded(path, 512 * 1024 * 1024)?;
    if tiff_page_count(&bytes) != Some(1) {
        return Err(MediaError::BadCodec);
    }
    let (width, height) = tiff_stored_size(&bytes).ok_or(MediaError::BadCodec)?;
    fits(width, height)?;
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
        shown_stream: 0,
        motion_cover: false,
        pages: false,
    })
}

fn decode_tiff_player(path: &Path, on_frame: &mut impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    let bytes = read_bounded(path, 512 * 1024 * 1024)?;
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
        shown_stream: 0,
        motion_cover: false,
        pages: false,
    };
    let mut frame = None;
    let stats = decode_video(path, &stored, false, &mut |_, image| {
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
            shown_stream: 0,
            motion_cover: false,
            pages: false,
        },
    })
}

fn oriented_still_size(width: u32, height: u32, tag: u8) -> (u32, u32) {
    if matches!(tag, 5..=8) {
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
    // The header gives the size. The pixels are decoded once, when the scan reads them.
    let (width, height) = still_size(path)?;
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
        shown_stream: 0,
        motion_cover: false,
        pages: false,
    })
}

fn still_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(MAX_PIXELS * 4 + 64 * 1024 * 1024);
    limits
}

/// A still's displayed size from its header, turned by its orientation tag.
fn still_size(path: &Path) -> Option<(u32, u32)> {
    let mut reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    reader.no_limits();
    let mut decoder = reader.into_decoder().ok()?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let mut orientation = decoder.orientation().ok()?;
    if orientation == Orientation::NoTransforms {
        orientation = webp_exif_orientation(path).or_else(|| png_exif_orientation(path)).unwrap_or(orientation);
    }
    let turned = matches!(
        orientation,
        Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
    );
    Some(if turned { (h, w) } else { (w, h) })
}

/// Decodes a still and applies its camera orientation tag when the file carries one.
/// A picture over `MAX_PIXELS` is refused before its pixels are allocated.
fn open_oriented(path: &Path) -> image::ImageResult<RgbImage> {
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    reader.limits(still_limits());
    let mut decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(image::ImageError::Limits(image::error::LimitError::from_kind(image::error::LimitErrorKind::DimensionError)));
    }
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

/// True when a JPEG ends before its end-of-image marker. Segments are skipped
/// by their lengths and scan data byte by byte, so data appended after the
/// marker, like a motion photo's clip, is not read. Not a JPEG: false.
pub fn jpeg_cut_short(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut bytes = BufReader::new(file).bytes().map_while(Result::ok);
    if (bytes.next(), bytes.next()) != (Some(0xFF), Some(0xD8)) {
        return false;
    }
    let mut pending = None;
    loop {
        let marker = match pending.take() {
            Some(marker) => marker,
            None => {
                // A marker is 0xFF, any fill 0xFFs, then its code.
                if bytes.by_ref().find(|&b| b == 0xFF).is_none() {
                    return true;
                }
                match bytes.by_ref().find(|&b| b != 0xFF) {
                    Some(code) => code,
                    None => return true,
                }
            }
        };
        match marker {
            0xD9 => return false,
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }
        let (Some(high), Some(low)) = (bytes.next(), bytes.next()) else {
            return true;
        };
        let length = usize::from(high) << 8 | usize::from(low);
        if length < 2 || bytes.by_ref().take(length - 2).count() != length - 2 {
            return true;
        }
        if marker != 0xDA {
            continue;
        }
        // Scan data runs to the next marker that is not a stuffed zero or a restart.
        loop {
            if bytes.by_ref().find(|&b| b == 0xFF).is_none() {
                return true;
            }
            match bytes.by_ref().find(|&b| b != 0xFF) {
                Some(0x00 | 0xD0..=0xD7) => {}
                Some(code) => {
                    pending = Some(code);
                    break;
                }
                None => return true,
            }
        }
    }
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

/// Each chunk of a WebP: tag, payload offset, payload size. Read by seeking,
/// so a large file costs a few small reads.
/// (tag, payload offset, payload size).
type Chunk = ([u8; 4], u64, u64);

fn webp_chunks(path: &Path) -> Option<(File, Vec<Chunk>)> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut header = [0u8; 12];
    file.read_exact(&mut header).ok()?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WEBP" {
        return None;
    }
    let mut chunks = Vec::new();
    let mut pos = 12u64;
    while pos + 8 <= len {
        file.seek(SeekFrom::Start(pos)).ok()?;
        let mut head = [0u8; 8];
        file.read_exact(&mut head).ok()?;
        let tag = [head[0], head[1], head[2], head[3]];
        let size = u64::from(u32::from_le_bytes([head[4], head[5], head[6], head[7]]));
        let start = pos + 8;
        if start + size > len || chunks.len() as u64 > MAX_STILL_FRAMES * 2 {
            return None;
        }
        chunks.push((tag, start, size));
        pos = start + size + (size & 1);
    }
    Some((file, chunks))
}

fn read_at(file: &mut File, offset: u64, len: u64) -> Option<Vec<u8>> {
    if len > MAX_HEADER_BYTES {
        return None;
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; len as usize];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// True when a WebP carries an animation chunk or the animation flag.
fn animated_webp(path: &Path) -> bool {
    let Some((mut file, chunks)) = webp_chunks(path) else {
        return false;
    };
    chunks.iter().any(|(tag, start, size)| {
        tag == b"ANIM" || tag == b"ANMF" || (tag == b"VP8X" && *size >= 1 && read_at(&mut file, *start, 1).is_some_and(|b| b[0] & 0x02 != 0))
    })
}

/// Seconds each animated-WebP picture stays up, from the frame headers.
/// A duration of 0 is a tenth of a second, which is how a player shows it.
fn webp_frame_delays(path: &Path) -> Option<Vec<f64>> {
    let (mut file, chunks) = webp_chunks(path)?;
    let mut delays = Vec::new();
    for (tag, start, size) in chunks {
        if &tag != b"ANMF" || size < 16 {
            continue;
        }
        let head = read_at(&mut file, start, 16)?;
        let millis = u32::from(head[12]) | u32::from(head[13]) << 8 | u32::from(head[14]) << 16;
        delays.push(if millis == 0 { 0.1 } else { f64::from(millis) / 1000.0 });
    }
    Some(delays)
}

/// The canvas size and frame delays, from the headers. No picture is decoded.
fn probe_animated_webp(path: &Path) -> Result<Probe, MediaError> {
    let delays = webp_frame_delays(path).filter(|d| !d.is_empty()).ok_or(MediaError::BadCodec)?;
    let (mut file, chunks) = webp_chunks(path).ok_or(MediaError::BadCodec)?;
    let (_, start, _) = chunks.iter().find(|(tag, _, size)| tag == b"VP8X" && *size >= 10).ok_or(MediaError::BadCodec)?;
    let head = read_at(&mut file, *start, 10).ok_or(MediaError::BadCodec)?;
    let width = 1 + (u32::from(head[4]) | u32::from(head[5]) << 8 | u32::from(head[6]) << 16);
    let height = 1 + (u32::from(head[7]) | u32::from(head[8]) << 8 | u32::from(head[9]) << 16);
    fits(width, height)?;
    let count = delays.len() as u64;
    if count > MAX_STILL_FRAMES {
        return Err(MediaError::TooLarge);
    }
    let turned = matches!(
        webp_exif_orientation(path),
        Some(Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH)
    );
    let (width, height) = if turned { (height, width) } else { (width, height) };
    let duration: f64 = delays.iter().sum();
    let moving = count > 1;
    let fps = if duration > 0.0 { count as f64 / duration } else { 0.0 };
    if moving && !(fps > 0.0) {
        return Err(MediaError::BadCodec);
    }
    Ok(Probe {
        width,
        height,
        fps: if moving { fps } else { 0.0 },
        duration_sec: if moving { duration } else { 0.0 },
        frames: count,
        frames_exact: true,
        container_created: None,
        video: moving,
        square_pixels: false,
        shown_stream: 0,
        motion_cover: false,
        pages: false,
    })
}

/// Every composited picture, one at a time.
fn decode_animated_webp(path: &Path, on_frame: &mut impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    let info = probe_animated_webp(path)?;
    let file = File::open(path).map_err(|_| MediaError::BadCodec)?;
    let decoder = image::codecs::webp::WebPDecoder::new(BufReader::new(file)).map_err(|_| MediaError::BadCodec)?;
    if !decoder.has_animation() {
        return Err(MediaError::BadCodec);
    }
    let orientation = webp_exif_orientation(path).unwrap_or(Orientation::NoTransforms);
    let mut decoded = 0u64;
    let mut clean = true;
    for frame in decoder.into_frames() {
        let frame = frame.map_err(|_| MediaError::BadCodec)?;
        let mut image = DynamicImage::ImageRgba8(frame.into_buffer());
        if orientation != Orientation::NoTransforms {
            image.apply_orientation(orientation);
        }
        let keep = on_frame(decoded, &image.to_rgb8());
        decoded += 1;
        if !keep {
            clean = false;
            break;
        }
    }
    Ok(DecodeStats { frames_decoded: decoded, clean: clean && decoded == info.frames, probe: info })
}

/// Orientation from a WebP EXIF chunk. The payload is a TIFF header, sometimes
/// behind the six-byte marker a JPEG uses.
fn webp_exif_orientation(path: &Path) -> Option<Orientation> {
    let (mut file, chunks) = webp_chunks(path)?;
    let (_, start, size) = chunks.into_iter().find(|(tag, _, _)| tag == b"EXIF")?;
    orientation_in_exif(&read_at(&mut file, start, size)?)
}

/// Orientation from a PNG eXIf chunk. The payload is a TIFF header, sometimes
/// behind the six-byte marker a JPEG uses. Chunks are walked by seeking.
fn png_exif_orientation(path: &Path) -> Option<Orientation> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut signature = [0u8; 8];
    file.read_exact(&mut signature).ok()?;
    if &signature != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut pos = 8u64;
    while pos + 12 <= len {
        file.seek(SeekFrom::Start(pos)).ok()?;
        let mut head = [0u8; 8];
        file.read_exact(&mut head).ok()?;
        let size = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
        let start = pos + 8;
        if start + size + 4 > len {
            return None;
        }
        match &head[4..8] {
            b"eXIf" => return orientation_in_exif(&read_at(&mut file, start, size)?),
            b"IEND" => return None,
            _ => pos = start + size + 4,
        }
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
    // A file can carry a small track before the picture a player shows. `v:0` is
    // that first track. The player shows a default track, and the largest of those.
    let shown = shown_video_stream(path)?;
    let selector = format!("v:{shown}");
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            &selector,
            "-show_entries",
            "stream=width,height,sample_aspect_ratio,avg_frame_rate,r_frame_rate,codec_name,nb_frames,duration:stream_tags=DURATION:stream_side_data=rotation",
            "-show_entries",
            "format=duration:format_tags=creation_time",
            "-show_frames",
            "-read_intervals",
            "%+#1",
            "-show_entries",
            "frame=width,height,duration_time,pkt_duration_time:frame_side_data=rotation",
            "-of",
            "json",
        ])
        .args(["-protocol_whitelist", "file"])
        .arg(ffmpeg_input(path))
        .watched_output()
        .map_err(|_| MediaError::BadCodec)?;
    if !output.status.success() {
        return Err(MediaError::BadCodec);
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| MediaError::BadCodec)?;
    let stream = value.get("streams").and_then(|s| s.get(0)).ok_or(MediaError::BadCodec)?;
    let header_width = json_u32(stream.get("width")).ok_or(MediaError::BadCodec)?;
    let header_height = json_u32(stream.get("height")).ok_or(MediaError::BadCodec)?;
    // The header crop can be a few pixels smaller than the picture this decoder
    // writes. The first frame is that picture. Allocating the header size asks
    // for a short buffer and refuses a file a player shows.
    let width = json_u32(value.pointer("/frames/0/width"))
        .filter(|n| *n > 0)
        .unwrap_or(header_width);
    let height = json_u32(value.pointer("/frames/0/height"))
        .filter(|n| *n > 0)
        .unwrap_or(header_height);
    // The average is the rate a player uses. Theora and IVF leave it at 0/0
    // and put the real rate on r_frame_rate. A timebase is not that rate.
    // An animated PNG can leave the average at 0/0 and put a 100000 timebase
    // on r_frame_rate. The first picture's own duration is the rate a player uses.
    let fps = {
        let listed = stream_frame_rate(stream);
        if listed > 0.0 {
            listed
        } else {
            frame_duration_rate(&value).unwrap_or(0.0)
        }
    };
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
    // The scan allocates one RGB buffer of this displayed size. A size that does
    // not fit is a refusal, not a plan for a frame that cannot be decoded.
    if rgb_frame_len(width, height).is_none() {
        return Err(MediaError::BadCodec);
    }
    // The container duration includes audio that continues after the pictures.
    // Matroska and WebM put the picture length in the track's DURATION tag.
    // MP4 puts it on the video stream. The frame estimate uses that length.
    let picture = picture_duration(stream, duration);
    let presented = presented_frames(picture, fps);
    // nb_frames counts every stored sample. An edit list can hide some of them.
    // The stream duration is the length a player shows.
    // Matroska omits nb_frames. AVI can store a nominal rate instead, so the
    // header count is larger than the pictures. The packets are the pictures.
    // A smaller header count is left alone: that is the edit list.
    let read = video_packets(path, shown);
    let packets = read.as_ref().map(|read| read.stored);
    let header_matches_packets = match (nb, packets) {
        (Some(listed), Some(counted)) => counted == 0 || counted >= listed,
        (Some(_), None) => true,
        (None, _) => false,
    };
    let shown_packets = read.as_ref().map(|read| read.shown);
    let stored = match (nb, packets) {
        (Some(listed), Some(counted)) if counted > 0 && counted < listed => Some(counted),
        // A header that counts fewer pictures than the file stores is not the
        // file. The pictures a player lists are. Trusting the header would stop
        // the decode early and call the rest of the file scanned.
        (Some(listed), Some(counted)) if counted > listed && shown_packets.is_some_and(|shown| shown > listed) => shown_packets,
        (Some(listed), _) => Some(listed),
        (None, packets) => packets,
    };
    // A hold makes the average rate the packet count over the duration, so
    // duration times that rate rounds back to the packets. An edit list can
    // still hide one of those packets. The frames a player lists are the pictures.
    // A short packet count is left alone: that file was cut, and the header
    // still names samples that are no longer there.
    let stored = match (stored, packets, read.as_ref().map(|read| read.shown).filter(|n| *n > 0)) {
        (Some(listed), Some(counted), Some(visible))
            if header_matches_packets && counted == listed && visible > 0 && visible < listed =>
        {
            Some(visible)
        }
        (stored, _, _) => stored,
    };
    let frames = match (stored, presented) {
        (Some(exact), Some(presented)) if header_matches_packets && presented > 0 && presented < exact => presented,
        (Some(exact), _) => exact,
        (None, Some(presented)) => presented,
        (None, None) => return Err(MediaError::BadCodec),
    };
    Ok(Probe {
        width,
        height,
        fps,
        duration_sec: if picture > 0.0 { picture } else { duration },
        frames,
        frames_exact: stored.is_some(),
        container_created: created,
        video: true,
        square_pixels,
        shown_stream: shown,
        motion_cover: false,
        pages: false,
    })
}

/// The `v:N` index of the video a player shows.
///
/// Tracks marked default win. Among those, the largest picture wins. An earlier
/// track wins a tie. A file with no default track uses every video track.
fn shown_video_stream(path: &Path) -> Result<usize, MediaError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height:stream_disposition=default",
            "-of",
            "json",
        ])
        .args(["-protocol_whitelist", "file"])
        .arg(ffmpeg_input(path))
        .watched_output()
        .map_err(|_| MediaError::BadCodec)?;
    if !output.status.success() {
        return Err(MediaError::BadCodec);
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| MediaError::BadCodec)?;
    let streams = value.get("streams").and_then(|s| s.as_array()).ok_or(MediaError::BadCodec)?;
    let mut videos = Vec::new();
    for stream in streams {
        if stream.get("codec_type").and_then(|v| v.as_str()) != Some("video") {
            continue;
        }
        let width = json_u32(stream.get("width")).unwrap_or(0);
        let height = json_u32(stream.get("height")).unwrap_or(0);
        let default_track = stream.pointer("/disposition/default").and_then(|v| v.as_i64()) == Some(1);
        videos.push((u64::from(width) * u64::from(height), default_track));
    }
    if videos.is_empty() {
        return Err(MediaError::BadCodec);
    }
    let any_default = videos.iter().any(|(_, default_track)| *default_track);
    let mut chosen: Option<(usize, u64)> = None;
    for (index, (area, default_track)) in videos.iter().enumerate() {
        if any_default && !default_track {
            continue;
        }
        if chosen.map(|(_, best)| *area > best).unwrap_or(true) {
            chosen = Some((index, *area));
        }
    }
    chosen.map(|(index, _)| index).ok_or(MediaError::BadCodec)
}

/// The shown track as the demuxer stores it. Reading packets does not decode,
/// so a long video is counted in the time it takes to read the file.
struct Packets {
    /// Every stored picture. One packet is one picture for the tracks this scan reads.
    /// An AVI header can count a nominal rate instead, so this count wins when it is smaller.
    stored: u64,
    /// Pictures a player lists. An edit list marks the samples it hides as discarded.
    shown: u64,
    /// Display time of each shown picture, in order. `None` when a packet has no
    /// time or two share one, because then only the decoder knows the order.
    times: Option<Vec<f64>>,
    /// How long the last shown picture stays up.
    last_duration: Option<f64>,
}

fn video_packets(path: &Path, shown: usize) -> Option<Packets> {
    let selector = format!("v:{shown}");
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            &selector,
            "-show_entries",
            "packet=pts_time,dts_time,duration_time,flags",
            "-of",
            "csv=p=0",
        ])
        .args(["-protocol_whitelist", "file"])
        .arg(ffmpeg_input(path))
        .watched_output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let mut stored = 0u64;
    let mut kept: Vec<(Option<f64>, Option<f64>)> = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        stored += 1;
        let mut fields = line.split(',');
        let pts = csv_f64(fields.next());
        let dts = csv_f64(fields.next());
        let duration = csv_f64(fields.next());
        let flags = fields.next().unwrap_or("");
        if flags.contains('D') {
            continue;
        }
        kept.push((pts.or(dts), duration));
    }
    if stored == 0 {
        return None;
    }
    let mut timed: Option<Vec<(f64, Option<f64>)>> =
        kept.iter().map(|(time, duration)| time.map(|time| (time, *duration))).collect();
    if let Some(list) = timed.as_mut() {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        if list.windows(2).any(|pair| pair[1].0 <= pair[0].0) {
            timed = None;
        }
    }
    let last_duration = timed.as_ref().and_then(|list| list.last()).and_then(|(_, duration)| *duration);
    Some(Packets {
        stored,
        shown: kept.len() as u64,
        times: timed.map(|list| list.into_iter().map(|(time, _)| time).collect()),
        last_duration,
    })
}

fn csv_f64(field: Option<&str>) -> Option<f64> {
    let text = field?.trim();
    if text.is_empty() || text == "N/A" {
        return None;
    }
    text.parse().ok().filter(|value: &f64| value.is_finite())
}

/// How many pictures `picture` seconds at `fps` is. `None` when that count does not
/// fit, so a broken duration is a refusal instead of a plan for every integer.
fn presented_frames(picture: f64, fps: f64) -> Option<u64> {
    if !(picture > 0.0) || !picture.is_finite() || !fps.is_finite() || fps <= 0.0 {
        return Some(0);
    }
    let pictures = picture * fps;
    if !pictures.is_finite() || pictures >= u64::MAX as f64 {
        return None;
    }
    Some(pictures.round() as u64)
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

/// Byte length of one RGB24 frame. `None` when that buffer does not fit, or the size is zero.
fn rgb_frame_len(width: u32, height: u32) -> Option<usize> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    let bytes = pixels.checked_mul(3)?;
    if bytes == 0 { None } else { Some(bytes) }
}

/// `turn` lets FFmpeg turn the picture by the stream's display rotation. A
/// picture whose orientation tag the scan applies itself is read as stored:
/// newer FFmpeg also reads image tags, and the picture would turn twice.
fn decode_video(path: &Path, info: &Probe, turn: bool, on_frame: &mut impl FnMut(u64, &RgbImage) -> bool) -> Result<DecodeStats, MediaError> {
    // Checked before FFmpeg starts. The unchecked product panics in debug when
    // both sides are near u32::MAX, and a wrapped length would allocate the wrong buffer.
    let frame_bytes = rgb_frame_len(info.width, info.height).ok_or(MediaError::BadCodec)?;
    let mut command = Command::new("ffmpeg");
    let mapped = format!("0:v:{}", info.shown_stream);
    command.args(["-v", "error", "-xerror", "-protocol_whitelist", "file"]);
    if !turn {
        command.arg("-noautorotate");
    }
    command.arg("-i").arg(ffmpeg_input(path));
    // The same track the probe measured. The default map can be a different one.
    command.args(["-map", &mapped]);
    if info.square_pixels {
        command.args(["-vf", "scale=iw*sar:ih,setsar=1"]);
    }
    // A picture stays up until the next one. FFmpeg's default decoder repeats
    // that picture to fill a steady rate, so the first `nb_frames` buffers can
    // all be the picture that stays up. The pictures after it never appear.
    // One buffer per stored picture is the file.
    command.args(["-fps_mode", "passthrough"]);
    let mut child = command
        .args(["-an", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| MediaError::BadCodec)?;
    // FFmpeg blocks when its error pipe fills. Read it on its own thread, keeping the tail.
    let stderr = child.stderr.take().ok_or(MediaError::BadCodec)?;
    let errors = std::thread::spawn(move || {
        let mut tail = Vec::new();
        let mut reader = BufReader::new(stderr);
        let mut chunk = [0u8; 4096];
        while let Ok(n) = reader.read(&mut chunk) {
            if n == 0 {
                break;
            }
            tail.extend_from_slice(&chunk[..n]);
            if tail.len() > 16 * 1024 {
                tail.drain(..tail.len() - 16 * 1024);
            }
        }
        tail
    });
    let mut stdout = child.stdout.take().ok_or(MediaError::BadCodec)?;
    let child = Arc::new(Mutex::new(child));
    let (beat, dog) = watch(&child, STALL);
    let mut decoded = 0u64;
    let mut stopped = false;
    let mut extra = 0u64;
    let mut torn = false;
    let mut buf = vec![0u8; frame_bytes];
    loop {
        let mut filled = 0;
        while filled < frame_bytes {
            match stdout.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => {
                    filled += n;
                    let _ = beat.send(());
                }
                Err(_) => {
                    torn = true;
                    break;
                }
            }
        }
        if filled == 0 || torn {
            break;
        }
        if filled != frame_bytes {
            torn = true;
            break;
        }
        // FFmpeg can repeat a one-frame GIF's picture; those repeats are not pictures
        // of the file. Past any other count the pictures are real and are scanned:
        // the count that disagrees then makes the scan incomplete.
        if info.frames_exact && info.frames == 1 && decoded >= 1 {
            extra += 1;
            continue;
        }
        let image = RgbImage::from_raw(info.width, info.height, std::mem::take(&mut buf)).ok_or(MediaError::BadCodec)?;
        let keep = on_frame(decoded, &image);
        buf = image.into_raw();
        decoded += 1;
        if !keep {
            stopped = true;
            break;
        }
    }
    drop(beat);
    let stalled = dog.join().unwrap_or(true);
    let mut child = child.lock().map_err(|_| MediaError::BadCodec)?;
    if stopped || torn {
        let _ = child.kill();
    }
    let exited = child.wait().map(|status| status.success()).unwrap_or(false);
    // A stalled FFmpeg was killed, so it did not finish.
    let finished = exited && !stalled;
    drop(child);
    let _ = errors.join();
    if decoded == 0 && !stopped {
        return Err(MediaError::BadCodec);
    }
    let repeats_of_one = info.frames == 1 && decoded == 1;
    let clean = !stopped && !torn && finished && (extra == 0 || repeats_of_one);
    Ok(DecodeStats { frames_decoded: decoded, clean, probe: info.clone() })
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

/// `scale=iw*sar:ih` drops the fraction. Rounding up asks for a pixel the
/// decoder does not write, and the short frame refuses the file.
fn square_width(width: u32, num: u32, den: u32) -> u32 {
    let wide = u64::from(width) * u64::from(num) / u64::from(den);
    u32::try_from(wide).unwrap_or(width).max(1)
}

/// Pictures per second from the average, or from `r_frame_rate` when the
/// average is missing. `parse_ratio` rejects 0 and a timebase of 1000 or more.
fn stream_frame_rate(stream: &Value) -> f64 {
    frame_rate_field(stream.get("avg_frame_rate"))
        .or_else(|| frame_rate_field(stream.get("r_frame_rate")))
        .unwrap_or(0.0)
}

/// Pictures per second from the first picture's duration. Zero is not a rate,
/// and a timebase of 1000 or more is not one either.
fn frame_duration_rate(value: &Value) -> Option<f64> {
    let duration = json_f64(value.pointer("/frames/0/duration_time"))
        .filter(|duration| *duration > 0.0)
        .or_else(|| {
            json_f64(value.pointer("/frames/0/pkt_duration_time")).filter(|duration| *duration > 0.0)
        })?;
    if !duration.is_finite() {
        return None;
    }
    let fps = 1.0 / duration;
    if fps > 0.0 && fps < 1000.0 { Some(fps) } else { None }
}

fn frame_rate_field(value: Option<&Value>) -> Option<f64> {
    value.and_then(|rate| rate.as_str()).and_then(parse_ratio)
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

/// Per-picture display times. A GIF uses its own blocks. A video uses the
/// timestamps a player shows. A still has none.
pub fn picture_delays(path: &Path) -> Option<Vec<f64>> {
    if let Some(delays) = gif_delays(path) {
        return Some(delays);
    }
    if let Some(delays) = webp_delays(path) {
        return Some(delays);
    }
    video_frame_delays(path)
}

/// Seconds each animated-WebP picture stays up. A still has none.
fn webp_delays(path: &Path) -> Option<Vec<f64>> {
    webp_frame_delays(path).filter(|delays| delays.len() >= 2)
}

/// Seconds each stored picture stays up, from the gaps between display times.
/// The last picture uses its own duration. A missing or backwards clock has no list.
fn video_frame_delays(path: &Path) -> Option<Vec<f64>> {
    let shown = shown_video_stream(path).ok()?;
    if let Some(read) = video_packets(path, shown) {
        if let Some(times) = read.times {
            return delays_from_times(&times, read.last_duration);
        }
    }
    let selector = format!("v:{shown}");
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            &selector,
            "-show_frames",
            "-show_entries",
            "frame=best_effort_timestamp_time,duration_time",
            "-of",
            "json",
        ])
        .args(["-protocol_whitelist", "file"])
        .arg(ffmpeg_input(path))
        .watched_output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    let frames = value.get("frames").and_then(|frames| frames.as_array())?;
    if frames.len() < 2 {
        return None;
    }
    let mut times = Vec::with_capacity(frames.len());
    for frame in frames {
        times.push(json_f64(frame.get("best_effort_timestamp_time"))?);
    }
    delays_from_times(&times, json_f64(frames.last().and_then(|frame| frame.get("duration_time"))))
}

fn delays_from_times(times: &[f64], last_duration: Option<f64>) -> Option<Vec<f64>> {
    if times.len() < 2 || times.iter().any(|time| !time.is_finite() || *time < 0.0) {
        return None;
    }
    let mut delays = Vec::with_capacity(times.len());
    for pair in times.windows(2) {
        let gap = pair[1] - pair[0];
        if !gap.is_finite() || gap < 0.0 {
            return None;
        }
        delays.push(gap);
    }
    let last = last_duration
        .filter(|delay| delay.is_finite() && *delay >= 0.0)
        .or_else(|| delays.last().copied())?;
    delays.push(last);
    if delays.iter().all(|delay| *delay == 0.0) {
        return None;
    }
    Some(delays)
}

fn json_f64(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    let text = value.as_str()?;
    if text.is_empty() || text == "N/A" {
        return None;
    }
    text.parse().ok()
}

/// Per-frame display times for a GIF. A still or a video has none.
/// The phone has no ffprobe, so the times come from the graphic-control blocks.
/// A delay of 0 is a tenth of a second, which is how a player and the phone decoder show it.
pub fn gif_delays(path: &Path) -> Option<Vec<f64>> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 6];
    file.read_exact(&mut header).ok()?;
    if &header != b"GIF87a" && &header != b"GIF89a" {
        return None;
    }
    if file.metadata().ok()?.len() > 256 * 1024 * 1024 {
        return None;
    }
    let mut bytes = header.to_vec();
    file.read_to_end(&mut bytes).ok()?;
    gif_delays_in(&bytes)
}

fn gif_delays_in(data: &[u8]) -> Option<Vec<f64>> {
    if data.len() < 13 || (&data[0..6] != b"GIF87a" && &data[0..6] != b"GIF89a") {
        return None;
    }
    let packed = *data.get(10)?;
    let mut index = 13usize;
    if packed & 0x80 != 0 {
        let count = 1usize << ((packed & 7) + 1);
        index = index.checked_add(count.checked_mul(3)?)?;
    }
    // The phone decoder starts at a tenth of a second and keeps the last control block.
    let mut delay_cs = 10u32;
    let mut delays = Vec::new();
    while index < data.len() {
        let block = *data.get(index)?;
        if block == 0x3B {
            break;
        }
        if block == 0x21 {
            if data.get(index + 1) == Some(&0xF9) && index + 7 < data.len() {
                let raw = u16::from_le_bytes([data[index + 4], data[index + 5]]) as u32;
                delay_cs = if raw == 0 { 10 } else { raw };
            }
            index = skip_gif_subblocks(data, index + 2)?;
            continue;
        }
        if block != 0x2C || index + 10 >= data.len() {
            return None;
        }
        let image_packed = data[index + 9];
        index += 10;
        if image_packed & 0x80 != 0 {
            let count = 1usize << ((image_packed & 7) + 1);
            index = index.checked_add(count.checked_mul(3)?)?;
        }
        index = index.checked_add(1)?;
        index = skip_gif_subblocks(data, index)?;
        delays.push(delay_cs as f64 / 100.0);
    }
    if delays.len() < 2 {
        return None;
    }
    Some(delays)
}

fn skip_gif_subblocks(data: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    while index < data.len() {
        let size = *data.get(index)? as usize;
        index += 1;
        if size == 0 {
            return Some(index);
        }
        index = index.checked_add(size)?;
        if index > data.len() {
            return None;
        }
    }
    None
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
    // A value below the 1970 offset is left alone: players read that range as a
    // Unix time, so subtracting the offset would call a 2026 file a 1960 file.
    if created < 2_082_844_800 {
        return None;
    }
    let unix = created - 2_082_844_800;
    // A creation time past what the clock can store is not a date. Adding it would panic.
    SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(unix))
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
    let ns = dateutc(&data, 0, data.len(), 0)?;
    let secs = ns.div_euclid(1_000_000_000);
    // DateUTC counts nanoseconds from 2001-01-01.
    // DateUTC counts nanoseconds from 2001-01-01. A date before 1970 is still a clock.
    let unix = secs.checked_add(978_307_200)?;
    crate::timeutil::system_time_from_unix(unix)
}

/// The `DateUTC` element, found in the header, segment, and info levels only.
/// A file that nests those levels deeper than a real one does is not read.
fn dateutc(data: &[u8], mut pos: usize, end: usize, depth: u32) -> Option<i64> {
    if depth > 4 {
        return None;
    }
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
            if let Some(ns) = dateutc(data, body, child_end, depth + 1) {
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

/// Every page of a TIFF, for tests.
#[cfg(test)]
fn read_tiff_pages(path: &Path) -> Result<Vec<RgbImage>, MediaError> {
    let info = probe_tiff(path)?;
    let mut pages = Vec::new();
    decode_tiff(path, info, &mut |_, page| {
        pages.push(page.clone());
        true
    })?;
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::open_oriented;

    #[test]
    fn a_jpeg_cut_short_is_told_from_a_whole_one_with_data_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let image = image::RgbImage::from_fn(320, 240, |x, y| image::Rgb([(x * 7 % 256) as u8, (y * 13 % 256) as u8, ((x ^ y) % 256) as u8]));
        let whole = dir.path().join("whole.jpg");
        image.save(&whole).unwrap();
        let bytes = std::fs::read(&whole).unwrap();
        assert!(!super::jpeg_cut_short(&whole));
        let cut = dir.path().join("cut.jpg");
        std::fs::write(&cut, &bytes[..bytes.len() / 4]).unwrap();
        assert!(super::jpeg_cut_short(&cut));
        // A motion photo's clip after the end marker, with stray marker bytes in it.
        let appended = dir.path().join("appended.jpg");
        std::fs::write(&appended, [bytes.as_slice(), &[0xFF, 0xD8, 0xFF, 0xDA, 0x00]].concat()).unwrap();
        assert!(!super::jpeg_cut_short(&appended));
        // A progressive JPEG has several scans.
        let progressive = dir.path().join("progressive.jpg");
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(&whole)
            .args(["-c:v", "mjpeg", "-q:v", "3", "-huffman", "optimal", "-flags", "+ilme"])
            .arg(&progressive)
            .status()
            .is_ok_and(|s| s.success());
        if made {
            assert!(!super::jpeg_cut_short(&progressive));
        }
        assert!(!super::jpeg_cut_short(&dir.path().join("missing.jpg")));
    }

    #[cfg(unix)]
    fn sleeper() -> std::sync::Arc<std::sync::Mutex<std::process::Child>> {
        let child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        std::sync::Arc::new(std::sync::Mutex::new(child))
    }

    #[cfg(unix)]
    #[test]
    fn a_silent_program_is_stopped_and_a_beating_one_is_not() {
        use std::time::{Duration, Instant};
        let child = sleeper();
        let start = Instant::now();
        let (_beat, dog) = super::watch(&child, Duration::from_millis(200));
        assert!(dog.join().unwrap(), "the silent program was stopped");
        assert!(!child.lock().unwrap().wait().unwrap().success());
        assert!(start.elapsed() < Duration::from_secs(5));

        let child = sleeper();
        let (beat, dog) = super::watch(&child, Duration::from_millis(200));
        for _ in 0..6 {
            std::thread::sleep(Duration::from_millis(50));
            beat.send(()).unwrap();
        }
        drop(beat);
        assert!(!dog.join().unwrap(), "a program that keeps writing is left alone");
        assert!(child.lock().unwrap().try_wait().unwrap().is_none());
        child.lock().unwrap().kill().unwrap();
    }

    #[test]
    fn a_sample_aspect_that_does_not_fit_keeps_the_coded_width() {
        // 320 * u32::MAX does not fit in the displayed width. The coded width stays.
        assert_eq!(super::square_width(320, u32::MAX, 1), 320);
        assert_eq!(super::square_width(320, 2, 1), 640);
    }

    #[test]
    fn a_frame_whose_pixels_do_not_fit_refuses() {
        // A displayed frame of u32::MAX by u32::MAX RGB does not fit in one buffer.
        // The old multiply overflowed after FFmpeg had already been started.
        let info = super::Probe {
            width: u32::MAX,
            height: u32::MAX,
            fps: 30.0,
            duration_sec: 1.0,
            frames: 1,
            frames_exact: true,
            container_created: None,
            video: true,
            square_pixels: false,
            shown_stream: 0,
            motion_cover: false,
            pages: false,
        };
        let result = super::decode_video(std::path::Path::new("missing-frame"), &info, true, &mut |_, _| true);
        assert!(matches!(result, Err(super::MediaError::BadCodec)));
    }

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
            Err(_) => {}
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
        assert_eq!(super::square_width(320, 4, 3), 426);
        assert_eq!(super::square_width(640, 45, 44), 654);
        assert_eq!(super::displayed_size(480, 320, 1, 2, true), (640, 480, true));
        assert_eq!(super::displayed_size(320, 480, 2, 1, true), (240, 320, true));
        assert_eq!(super::displayed_size(320, 480, 2, 1, false), (640, 480, true));
        assert_eq!(super::displayed_size(480, 640, 1, 1, true), (640, 480, false));
    }

    #[test]
    fn a_duration_that_does_not_fit_is_not_a_frame_count() {
        assert_eq!(super::presented_frames(2.0, 30.0), Some(60));
        assert_eq!(super::presented_frames(0.0, 30.0), Some(0));
        assert!(super::presented_frames(1e20, 30.0).is_none());
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
        // DateUTC is nanoseconds before 2001-01-01. 1960 is still a clock.
        fs::write(&mkv, matroska_clock(-1_293_926_400_000_000_000)).unwrap();
        let mkv_before = super::container_created(&mkv).expect("a 1960 matroska clock");
        assert_eq!(
            std::time::SystemTime::UNIX_EPOCH
                .duration_since(mkv_before)
                .expect("before 1970")
                .as_secs(),
            315_619_200
        );
        let now = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let warnings = crate::scan::media_warnings(Some(now), Some(mkv_before), now);
        assert!(
            warnings.iter().any(|line| line.is(crate::copy::TIMESTAMP_DISAGREE)),
            "{warnings:?}"
        );
        fs::write(dir.path().join("still.png"), []).unwrap();
        assert!(super::container_created(&dir.path().join("still.png")).is_none());
        fs::write(&mp4, movie_clock_wide(u64::MAX)).unwrap();
        assert!(super::container_created(&mp4).is_none());
    }

    #[test]
    fn a_gif_delay_of_zero_is_a_tenth_of_a_second() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&[0x80, 0, 0, 0, 0, 0, 255, 0, 0]);
        for delay in [4u16, 0, 4] {
            bytes.extend_from_slice(&[0x21, 0xF9, 0x04, 0x00]);
            bytes.extend_from_slice(&delay.to_le_bytes());
            bytes.extend_from_slice(&[0x00, 0x00]);
            bytes.push(0x2C);
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.push(0x00);
            bytes.extend_from_slice(&[2, 3, 0x04, 0x05, 0x00, 0]);
        }
        bytes.push(0x3B);
        assert_eq!(super::gif_delays_in(&bytes), Some(vec![0.04, 0.1, 0.04]));
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

    fn movie_clock_wide(created: u64) -> Vec<u8> {
        fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = ((8 + body.len()) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out
        }
        let mut header = vec![0u8; 100];
        header[0] = 1;
        header[4..12].copy_from_slice(&created.to_be_bytes());
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
