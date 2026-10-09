// SPDX-License-Identifier: Apache-2.0

//! C entry point for the iPhone, Mac, and Android shells.
//! The shells decode frames and pass a directory. They do not detect or compare.

use crate::estimate::{Coverage, DetectionSize, FormFactor};
use crate::hardware::resolve_execution;
use crate::invoke::invoke_json;
use crate::scan::{scan_path, MediaFacts, ScanRequest};
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::time::SystemTime;

#[no_mangle]
pub extern "C" fn ow_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

/// Shell argument list as `{"argv":[...]}`. Same JSON the CLI prints. The caller frees with `ow_string_free`.
#[no_mangle]
pub extern "C" fn ow_command(request: *const c_char) -> *mut c_char {
    let report = std::panic::catch_unwind(|| {
        if request.is_null() {
            return r#"{"status":"refused","summary":"The scan request was empty. Refusing."}"#.to_string();
        }
        let text = unsafe { CStr::from_ptr(request) }.to_string_lossy();
        invoke_json(&text)
    })
    .unwrap_or_else(|_| r#"{"status":"refused","summary":"The scan stopped. Refusing.","message":"The scan stopped. Refusing."}"#.to_string());
    CString::new(report)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

/// Scan request JSON in, scan report JSON out. The caller frees the result with `ow_string_free`.
#[no_mangle]
pub extern "C" fn ow_scan_request(request: *const c_char) -> *mut c_char {
    let report = std::panic::catch_unwind(|| scan_from_json(request)).unwrap_or_else(|_| {
        r#"{"status":"refused","summary":"The scan stopped. Refusing.","message":"The scan stopped. Refusing."}"#.to_string()
    });
    CString::new(report)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

fn scan_from_json(request: *const c_char) -> String {
    if request.is_null() {
        return r#"{"status":"refused","summary":"The scan request was empty. Refusing."}"#.into();
    }
    let text = unsafe { CStr::from_ptr(request) }.to_string_lossy();
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            return r#"{"status":"refused","summary":"The scan request was not JSON. Refusing."}"#
                .into()
        }
    };
    let req = match request_from_value(&value) {
        Ok(req) => req,
        Err(message) => {
            return serde_json::json!({"status":"refused","summary": message, "message": message})
                .to_string();
        }
    };
    let report = scan_path(&req, &mut |_| {});
    serde_json::to_string(&report)
        .unwrap_or_else(|_| r#"{"status":"incomplete","summary":"Incomplete."}"#.into())
}

fn request_from_value(value: &Value) -> Result<ScanRequest, String> {
    let input = PathBuf::from(value.get("input").and_then(|v| v.as_str()).unwrap_or(""));
    let bundles = PathBuf::from(value.get("bundles").and_then(|v| v.as_str()).unwrap_or(""));
    let bundle = value
        .get("bundle")
        .and_then(|v| v.as_str())
        .unwrap_or("fast")
        .to_string();
    let posters = PathBuf::from(value.get("posters").and_then(|v| v.as_str()).unwrap_or(""));
    let out = PathBuf::from(value.get("out").and_then(|v| v.as_str()).unwrap_or(""));
    let long_side = value
        .get("long_side")
        .and_then(|v| v.as_str())
        .unwrap_or("640");
    let coverage = value
        .get("coverage")
        .and_then(|v| v.as_str())
        .unwrap_or("complete");
    let form = value
        .get("form_factor")
        .and_then(|v| v.as_str())
        .unwrap_or("computer");
    let provider = value
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("cpu");
    let detection =
        DetectionSize::parse(long_side).ok_or("Detection size must be 320, 480, 640, or full.")?;
    let coverage = Coverage::parse(coverage).ok_or("Coverage must be complete or measured.")?;
    let form_factor = match form {
        "phone" => FormFactor::Phone,
        "computer" => FormFactor::Computer,
        _ => return Err("Form factor must be phone or computer.".into()),
    };
    let frames_dir = value
        .get("frames")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);
    let media = value
        .get("media")
        .and_then(|v| v.as_object())
        .map(|media| MediaFacts {
            width: media.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            height: media.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            fps: media.get("fps").and_then(|v| v.as_f64()).unwrap_or(0.0),
            frames: media.get("frames").and_then(|v| v.as_u64()).unwrap_or(0),
            duration_sec: media
                .get("duration_sec")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            video: media
                .get("video")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            container_unix: media.get("container_unix").and_then(|v| v.as_u64()),
        });
    Ok(ScanRequest {
        input,
        bundles_dir: bundles,
        bundle_id: bundle,
        posters_dir: posters,
        out_dir: out,
        detection,
        coverage,
        form_factor,
        execution: resolve_execution(provider),
        missing: true,
        wanted: true,
        abort_after_frames: None,
        frames_dir,
        media,
        now: SystemTime::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn a_missing_file_is_a_refusal_from_the_library_entry() {
        let request = serde_json::json!({
            "input": "/tmp/openworld-missing-file.png",
            "bundles": format!("{}/../../bundles", env!("CARGO_MANIFEST_DIR")),
            "bundle": "fast",
            "posters": "/tmp/openworld-missing-posters",
            "out": "/tmp/openworld-missing-out",
            "long_side": "640",
            "coverage": "complete",
            "form_factor": "computer"
        });
        let c = CString::new(request.to_string()).unwrap();
        let ptr = ow_scan_request(c.as_ptr());
        let text = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        ow_string_free(ptr);
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["status"], "refused");
        assert_ne!(value["summary"], "No candidate is not a clearance.");
    }

    #[test]
    fn a_bad_request_refuses_before_a_scan() {
        for text in [
            "{",
            r#"{"long_side":"100"}"#,
            r#"{"long_side":"640","coverage":"sometimes"}"#,
            r#"{"long_side":"640","coverage":"complete","form_factor":"tablet"}"#,
        ] {
            let c = CString::new(text).unwrap();
            let ptr = ow_scan_request(c.as_ptr());
            let body = unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned();
            ow_string_free(ptr);
            assert!(body.contains("refused"), "{body}");
            assert!(!body.contains("No candidate is not a clearance."), "{body}");
        }
    }

    #[test]
    fn a_platform_frame_request_scans_from_the_c_entry() {
        let dir = tempfile::tempdir().unwrap();
        let posters = dir.path().join("posters");
        crate::posters::write_fixture_pack(
            &posters,
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
        )
        .unwrap();
        let frames = dir.path().join("frames");
        std::fs::create_dir_all(&frames).unwrap();
        let input = dir.path().join("in.png");
        image::RgbImage::new(32, 32).save(&input).unwrap();
        std::fs::copy(&input, frames.join("frame_000000.png")).unwrap();
        let bundles = format!("{}/../../bundles", env!("CARGO_MANIFEST_DIR"));
        let request = serde_json::json!({
            "input": input,
            "bundles": bundles,
            "bundle": "fast",
            "posters": posters,
            "out": dir.path().join("out"),
            "long_side": "full",
            "coverage": "measured",
            "form_factor": "phone",
            "provider": "gpu",
            "frames": frames,
            "media": {
                "width": 32,
                "height": 32,
                "fps": 0.0,
                "frames": 1,
                "duration_sec": 0.0,
                "video": false,
                "container_unix": 1_000_000_000u64
            }
        });
        let c = CString::new(request.to_string()).unwrap();
        let ptr = ow_scan_request(c.as_ptr());
        let body = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        ow_string_free(ptr);
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["status"], "complete", "{body}");
        assert_eq!(value["summary"], "No candidate is not a clearance.");
        assert_eq!(value["coverage"], "measured");
    }
}
