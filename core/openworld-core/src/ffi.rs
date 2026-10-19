// SPDX-License-Identifier: Apache-2.0

//! The C interface the phone and desktop shells link (`include/openworld.h`).
//!
//! `ow_call` answers one typed JSON request. A scan whose frames the shell
//! decodes itself is a job: start it, push each decoded frame in order (or
//! skip the ones `ow_job_wants` declines), then finish it with how decoding
//! ended. A stop token ends the job from any thread: Cancel, the device
//! getting hot, the battery, or the app leaving the screen. The scan then
//! reports Incomplete, never a clearance.
//!
//! No panic crosses this boundary. Every returned string is freed with
//! `ow_string_free`.
//!
//! # Safety
//!
//! Every pointer is null or valid for the call: strings are NUL-terminated,
//! pixel buffers hold `stride * height` bytes (chroma planes their rows),
//! and a job is used by one thread at a time and not after `ow_job_finish`
//! or `ow_job_free`. A stop token may be used from any thread until freed.
//! The C header states the same contract; these functions check for null.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::copy::Line;
use crate::engine::{DecodeEnd, Event, StopFlag};
use crate::invoke;
use crate::scan::{open_pushed_job, Job, ScanRequest};
use image::RgbImage;
use serde::Deserialize;
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::SystemTime;

pub type OwEventFn = extern "C" fn(json: *const c_char, user: *mut c_void);

pub struct OwJob {
    job: Option<Job>,
    events: Option<(OwEventFn, usize)>,
    rgb: Vec<u8>,
}

pub struct OwStop(StopFlag);

fn to_c(text: String) -> *mut c_char {
    CString::new(text.replace('\0', "")).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

fn from_c(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok().map(str::to_string)
}

fn emit(events: Option<(OwEventFn, usize)>) -> impl FnMut(Event) {
    move |event: Event| {
        if let Some((callback, user)) = events {
            if let Ok(line) = serde_json::to_string(&event) {
                if let Ok(text) = CString::new(line) {
                    callback(text.as_ptr(), user as *mut c_void);
                }
            }
        }
    }
}

fn panicked() -> String {
    invoke::refusal(Line::of(crate::copy::R_INTERNAL)).to_string()
}

/// Answer one request. Returns JSON. Scan events, if any, go to `on_event`.
#[no_mangle]
pub extern "C" fn ow_call(request: *const c_char, on_event: Option<OwEventFn>, user: *mut c_void) -> *mut c_char {
    let events = on_event.map(|f| (f, user as usize));
    let answer = catch_unwind(AssertUnwindSafe(|| {
        let text = from_c(request).unwrap_or_default();
        invoke::call(&text, None, &mut emit(events)).to_string()
    }));
    to_c(answer.unwrap_or_else(|_| panicked()))
}

/// Start a scan of frames the shell will push. The request is a scan request
/// with `media` set. On refusal this returns null and writes the refusal JSON
/// to `refusal`, which the caller frees.
#[no_mangle]
pub extern "C" fn ow_job_start(request: *const c_char, on_event: Option<OwEventFn>, user: *mut c_void, refusal: *mut *mut c_char) -> *mut OwJob {
    let started = catch_unwind(AssertUnwindSafe(|| {
        let text = from_c(request).unwrap_or_default();
        let scan = scan_request(&text).map_err(|line| invoke::refusal(line).to_string())?;
        if !(scan.missing || scan.wanted) {
            return Err(invoke::refusal(Line::of(crate::copy::CHOOSE_CLASS)).to_string());
        }
        open_pushed_job(&scan, SystemTime::now()).map_err(|report| invoke::refusal(report.headline).to_string())
    }));
    let write = |message: String| {
        if !refusal.is_null() {
            unsafe { *refusal = to_c(message) };
        }
    };
    match started {
        Ok(Ok(job)) => Box::into_raw(Box::new(OwJob { job: Some(job), events: on_event.map(|f| (f, user as usize)), rgb: Vec::new() })),
        Ok(Err(message)) => {
            write(message);
            std::ptr::null_mut()
        }
        Err(_) => {
            write(panicked());
            std::ptr::null_mut()
        }
    }
}

/// A versioned scan request: `{"v": 1, "op": "scan", ...ScanRequest}`. `op` may be left out.
fn scan_request(text: &str) -> Result<ScanRequest, Line> {
    let mut value: serde_json::Value = serde_json::from_str(text).map_err(|_| Line::of(crate::copy::R_BAD_REQUEST))?;
    let object = value.as_object_mut().ok_or_else(|| Line::of(crate::copy::R_BAD_REQUEST))?;
    if object.remove("v").and_then(|v| v.as_u64()) != Some(invoke::API_VERSION) {
        return Err(Line::of(crate::copy::R_BAD_REQUEST));
    }
    if object.remove("op").is_some_and(|op| op != "scan") {
        return Err(Line::of(crate::copy::R_BAD_REQUEST));
    }
    serde_json::from_value(value).map_err(|_| Line::of(crate::copy::R_BAD_REQUEST))
}

fn with_job(job: *mut OwJob, f: impl FnOnce(&mut OwJob) -> bool) -> i32 {
    if job.is_null() {
        return 0;
    }
    let job = unsafe { &mut *job };
    i32::from(catch_unwind(AssertUnwindSafe(|| f(job))).unwrap_or(false))
}

/// 1 when frame `index` will be analyzed. A 0 means: skip it, without copying it.
#[no_mangle]
pub extern "C" fn ow_job_wants(job: *mut OwJob, index: u64) -> i32 {
    with_job(job, |j| j.job.as_mut().is_some_and(|job| job.engine().wants(index)))
}

/// Count frame `index` as decoded and not analyzed. Only after `ow_job_wants` said 0.
#[no_mangle]
pub extern "C" fn ow_job_skip(job: *mut OwJob, index: u64) -> i32 {
    with_job(job, |j| j.job.as_mut().is_some_and(|job| job.engine().skip(index)))
}

/// Pixel layouts for `ow_job_push_pixels`.
pub const OW_RGBA: u32 = 0;
pub const OW_BGRA: u32 = 1;
pub const OW_RGB: u32 = 2;

/// Push frame `index`, packed pixels. `rotation` is 0, 90, 180, or 270 degrees
/// clockwise: how far to turn the stored picture so it is shown upright.
/// Returns 0 when the scan has stopped; finish it then.
#[no_mangle]
pub extern "C" fn ow_job_push_pixels(
    job: *mut OwJob,
    index: u64,
    pixels: *const u8,
    width: u32,
    height: u32,
    stride: u32,
    layout: u32,
    rotation: u32,
) -> i32 {
    with_job(job, |j| {
        let bpp = if layout == OW_RGB { 3 } else { 4 };
        let (row_len, stride) = (width as usize * bpp, stride as usize);
        if pixels.is_null() || !frame_fits(width, height) || stride < row_len {
            return false;
        }
        // The last row ends at its pixels: a platform buffer need not pad it to the stride.
        let Some(len) = stride.checked_mul(height as usize - 1).and_then(|n| n.checked_add(row_len)) else {
            return false;
        };
        let src = unsafe { std::slice::from_raw_parts(pixels, len) };
        let mut rgb = std::mem::take(&mut j.rgb);
        rgb.clear();
        rgb.reserve(width as usize * height as usize * 3);
        for row in 0..height as usize {
            for px in src[row * stride..row * stride + row_len].chunks_exact(bpp) {
                match layout {
                    OW_BGRA => rgb.extend_from_slice(&[px[2], px[1], px[0]]),
                    _ => rgb.extend_from_slice(&px[..3]),
                }
            }
        }
        push(j, index, rgb, width, height, rotation)
    })
}

/// Push frame `index` as 8-bit YUV 4:2:0 planes, as MediaCodec and
/// AVFoundation hand them out. `uv_step` is the byte step between chroma
/// samples in a row (1 planar, 2 interleaved). `full_range` is 1 for 0-255
/// luma. `bt709` is 1 for HD and newer video, 0 for BT.601.
#[no_mangle]
pub extern "C" fn ow_job_push_yuv420(
    job: *mut OwJob,
    index: u64,
    y: *const u8,
    y_stride: u32,
    u: *const u8,
    v: *const u8,
    uv_stride: u32,
    uv_step: u32,
    width: u32,
    height: u32,
    full_range: i32,
    bt709: i32,
    rotation: u32,
) -> i32 {
    with_job(job, |j| {
        if y.is_null() || u.is_null() || v.is_null() || !frame_fits(width, height) || uv_step == 0 || (y_stride as usize) < width as usize {
            return false;
        }
        let (cw, ch) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
        let y_len = (y_stride as usize).checked_mul(height as usize - 1).and_then(|n| n.checked_add(width as usize));
        let uv_len = (uv_stride as usize)
            .checked_mul(ch - 1)
            .and_then(|n| n.checked_add((cw - 1).checked_mul(uv_step as usize)?))
            .and_then(|n| n.checked_add(1));
        let (Some(y_len), Some(uv_len)) = (y_len, uv_len) else {
            return false;
        };
        let (yp, up, vp) = unsafe {
            (std::slice::from_raw_parts(y, y_len), std::slice::from_raw_parts(u, uv_len), std::slice::from_raw_parts(v, uv_len))
        };
        let mut rgb = std::mem::take(&mut j.rgb);
        rgb.clear();
        rgb.reserve(width as usize * height as usize * 3);
        // Fixed-point coefficients, scaled by 1024.
        let (kr, kgu, kgv, kb) = if bt709 != 0 { (1613, 192, 479, 1901) } else { (1436, 352, 731, 1815) };
        for row in 0..height as usize {
            let yrow = &yp[row * y_stride as usize..];
            let crow = (row / 2) * uv_stride as usize;
            #[allow(clippy::needless_range_loop)]
            for col in 0..width as usize {
                let c = crow + (col / 2) * uv_step as usize;
                let (mut l, cb, cr) = (i32::from(yrow[col]), i32::from(up[c]) - 128, i32::from(vp[c]) - 128);
                let (cb, cr) = if full_range != 0 {
                    (cb, cr)
                } else {
                    l = ((l - 16) * 1192) >> 10;
                    ((cb * 1166) >> 10, (cr * 1166) >> 10)
                };
                let r = l + ((kr * cr) >> 10);
                let g = l - ((kgu * cb + kgv * cr) >> 10);
                let b = l + ((kb * cb) >> 10);
                rgb.extend_from_slice(&[r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8]);
            }
        }
        push(j, index, rgb, width, height, rotation)
    })
}

/// A pushed frame has pixels and is within the picture limit, checked before any copy.
fn frame_fits(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && u64::from(width) * u64::from(height) <= crate::decode::MAX_PIXELS
}

fn push(j: &mut OwJob, index: u64, rgb: Vec<u8>, width: u32, height: u32, rotation: u32) -> bool {
    let Some(image) = RgbImage::from_raw(width, height, rgb) else {
        return false;
    };
    let image = match rotation % 360 {
        90 => image::imageops::rotate90(&image),
        180 => image::imageops::rotate180(&image),
        270 => image::imageops::rotate270(&image),
        _ => image,
    };
    let mut events = emit(j.events);
    let Some(job) = j.job.as_mut() else {
        return false;
    };
    let keep = job.engine().push(index, &image, &mut events);
    j.rgb = image.into_raw();
    keep
}

/// A handle that stops this job from any thread. Free it with `ow_stop_free`.
#[no_mangle]
pub extern "C" fn ow_job_stop_token(job: *mut OwJob) -> *mut OwStop {
    if job.is_null() {
        return std::ptr::null_mut();
    }
    let job = unsafe { &*job };
    match &job.job {
        Some(inner) => Box::into_raw(Box::new(OwStop(inner.stop.clone()))),
        None => std::ptr::null_mut(),
    }
}

/// `reason` is "cancel", "heat", "battery", or "background".
#[no_mangle]
pub extern "C" fn ow_stop(token: *mut OwStop, reason: *const c_char) {
    if token.is_null() {
        return;
    }
    let token = unsafe { &*token };
    token.0.stop(StopFlag::parse(&from_c(reason).unwrap_or_default()));
}

#[no_mangle]
pub extern "C" fn ow_stop_free(token: *mut OwStop) {
    if !token.is_null() {
        drop(unsafe { Box::from_raw(token) });
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct EndRequest {
    /// The decoder reached the end of the pictures without an error.
    #[serde(default)]
    clean: bool,
}

/// End the job and free it. `end` is `{"clean": true}` when the decoder
/// reached the end without an error. Returns the report JSON.
#[no_mangle]
pub extern "C" fn ow_job_finish(job: *mut OwJob, end: *const c_char) -> *mut c_char {
    if job.is_null() {
        return to_c(panicked());
    }
    let owned = unsafe { Box::from_raw(job) };
    let answer = catch_unwind(AssertUnwindSafe(move || {
        let mut owned = owned;
        let end: EndRequest = from_c(end).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let Some(job) = owned.job.take() else {
            return panicked();
        };
        let declared = job.declared_frames;
        let report = job.finish(DecodeEnd { clean: end.clean, declared_frames: declared, reason: None }, &mut emit(owned.events));
        let mut value = serde_json::to_value(&report).unwrap_or_default();
        value["ok"] = (!report.is_refused()).into();
        value.to_string()
    }));
    to_c(answer.unwrap_or_else(|_| panicked()))
}

/// Free a job without a report. Its result folder stays until `delete` or `sweep`.
#[no_mangle]
pub extern "C" fn ow_job_free(job: *mut OwJob) {
    if !job.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(job) })));
    }
}

#[no_mangle]
pub extern "C" fn ow_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}

#[cfg(all(test, feature = "fixture"))]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn answer(ptr: *mut c_char) -> serde_json::Value {
        let text = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
        ow_string_free(ptr);
        serde_json::from_str(&text).unwrap()
    }

    extern "C" fn count_events(_json: *const c_char, user: *mut c_void) {
        unsafe { *(user as *mut u32) += 1 };
    }

    fn setup() -> (tempfile::TempDir, String, String) {
        let root = tempfile::tempdir().unwrap();
        let posters = root.path().join("posters");
        crate::posters::write_fixture_pack(&posters, SystemTime::now()).unwrap();
        let bundles = env!("CARGO_MANIFEST_DIR").to_string() + "/../../bundles";
        (root, posters.to_string_lossy().into(), bundles)
    }

    #[test]
    fn a_pushed_scan_finds_the_demo_marker_and_reports_once() {
        let (root, posters, bundles) = setup();
        let out = root.path().join("result");
        let request = serde_json::json!({
            "v": 1, "input": "", "bundles": bundles, "posters": posters, "out": out,
            "media": {"width": 640, "height": 480, "frames": 1}
        });
        let request = CString::new(request.to_string()).unwrap();
        let mut events = 0u32;
        let mut refusal = std::ptr::null_mut();
        let job = ow_job_start(request.as_ptr(), Some(count_events), &mut events as *mut u32 as *mut c_void, &mut refusal);
        assert!(!job.is_null(), "{}", answer(refusal));
        let scene = crate::fixture::demo_scene().image;
        let rgba: Vec<u8> = scene.pixels().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        assert_eq!(ow_job_wants(job, 0), 1);
        assert_eq!(ow_job_push_pixels(job, 0, rgba.as_ptr(), 640, 480, 640 * 4, OW_RGBA, 0), 1);
        let end = CString::new(r#"{"clean":true}"#).unwrap();
        let report = answer(ow_job_finish(job, end.as_ptr()));
        assert_eq!(report["status"], "complete", "{report}");
        assert_eq!(report["headline"]["code"], "possible_candidate");
        assert_eq!(report["candidates"].as_array().unwrap().len(), 1);
        assert!(events >= 2);
    }

    #[test]
    fn fewer_frames_than_promised_is_incomplete_and_a_stop_says_why() {
        let (root, posters, bundles) = setup();
        // A shell may push again and see the stop, skip, or stop delivering at once.
        for (stop, after, code) in [(None, "", "not_fully_decoded"), (Some("heat"), "push", "stopped_heat"), (Some("battery"), "skip", "stopped_battery"), (Some("background"), "", "stopped_background")] {
            let out = root.path().join(format!("r-{code}"));
            let request = serde_json::json!({
                "v": 1, "op": "scan", "bundles": bundles, "posters": posters, "out": out,
                "media": {"width": 64, "height": 48, "frames": 1800, "fps": 30.0, "video": true}
            });
            let request = CString::new(request.to_string()).unwrap();
            let job = ow_job_start(request.as_ptr(), None, std::ptr::null_mut(), std::ptr::null_mut());
            assert!(!job.is_null());
            let token = ow_job_stop_token(job);
            let gray = vec![200u8; 64 * 48 * 3];
            assert_eq!(ow_job_push_pixels(job, 0, gray.as_ptr(), 64, 48, 64 * 3, OW_RGB, 0), 1);
            if let Some(reason) = stop {
                let reason = CString::new(reason).unwrap();
                ow_stop(token, reason.as_ptr());
                match after {
                    "push" => assert_eq!(ow_job_push_pixels(job, 1, gray.as_ptr(), 64, 48, 64 * 3, OW_RGB, 0), 0),
                    "skip" => assert_eq!(ow_job_skip(job, 1), 0),
                    _ => {}
                }
            }
            ow_stop_free(token);
            let end = CString::new(format!(r#"{{"clean":{}}}"#, stop.is_none())).unwrap();
            let report = answer(ow_job_finish(job, end.as_ptr()));
            assert_eq!(report["status"], "incomplete");
            assert_eq!(report["reason"]["code"], code);
            assert!(report["disclosure"].as_array().unwrap().iter().all(|l| l["code"] != "no_clearance"));
        }
    }

    #[test]
    fn yuv_gray_is_gray_and_bad_input_is_refused() {
        let (root, posters, bundles) = setup();
        let request = serde_json::json!({
            "v": 1, "bundles": bundles, "posters": posters, "out": root.path().join("yuv"),
            "media": {"width": 4, "height": 2, "frames": 1}
        });
        let request = CString::new(request.to_string()).unwrap();
        let job = ow_job_start(request.as_ptr(), None, std::ptr::null_mut(), std::ptr::null_mut());
        let y = [128u8; 8];
        let uv = [128u8; 2];
        assert_eq!(ow_job_push_yuv420(job, 0, y.as_ptr(), 4, uv.as_ptr(), uv.as_ptr(), 2, 1, 4, 2, 1, 1, 0), 1);
        let unsafe_job = unsafe { &*job };
        assert!(unsafe_job.rgb.chunks(3).all(|p| p == [128, 128, 128]));
        assert_eq!(ow_job_push_yuv420(job, 1, std::ptr::null(), 4, uv.as_ptr(), uv.as_ptr(), 2, 1, 4, 2, 1, 1, 0), 0);
        ow_job_free(job);
        let bad = CString::new("{\"v\":1}").unwrap();
        let mut refusal = std::ptr::null_mut();
        assert!(ow_job_start(bad.as_ptr(), None, std::ptr::null_mut(), &mut refusal).is_null());
        assert_eq!(answer(refusal)["ok"], false);
        assert!(ow_job_start(std::ptr::null(), None, std::ptr::null_mut(), std::ptr::null_mut()).is_null());
    }

    #[test]
    fn a_padded_buffer_whose_last_row_is_not_padded_is_read_and_a_huge_frame_is_not() {
        let (root, posters, bundles) = setup();
        let request = serde_json::json!({
            "v": 1, "bundles": bundles, "posters": posters, "out": root.path().join("tight"),
            "media": {"width": 4, "height": 3, "frames": 2, "fps": 30.0, "video": true}
        });
        let request = CString::new(request.to_string()).unwrap();
        let job = ow_job_start(request.as_ptr(), None, std::ptr::null_mut(), std::ptr::null_mut());
        // Rows of 12 bytes at a stride of 16; the last row stops at its pixels.
        let mut pixels = vec![0u8; 16 * 2 + 12];
        for row in 0..3 {
            pixels[row * 16..row * 16 + 12].fill(77);
        }
        assert_eq!(ow_job_push_pixels(job, 0, pixels.as_ptr(), 4, 3, 16, OW_RGB, 0), 1);
        assert!(unsafe { &*job }.rgb.iter().all(|&b| b == 77));
        // Over the picture limit: refused before a byte is read.
        assert_eq!(ow_job_push_pixels(job, 1, pixels.as_ptr(), 20_000, 20_000, 60_000, OW_RGB, 0), 0);
        ow_job_free(job);
    }
}
