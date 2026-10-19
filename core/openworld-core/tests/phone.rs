// SPDX-License-Identifier: Apache-2.0

//! A phone decodes with its own platform decoder and pushes frames into a job.
//! The job checks the request before any frame, takes frames in order, and
//! finishes Incomplete when fewer frames arrive than the decoder promised.

mod common;

use common::*;
use image::RgbImage;
use openworld_core::copy::{self, Copy};
use openworld_core::ffi;
use openworld_core::report::{ScanReport, Status};
use openworld_core::scan::{estimate_for, open_pushed_job, Job, ScanRequest};
use openworld_core::{Coverage, DecodeEnd, MediaFacts};
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::fs;
use std::time::SystemTime;

fn facts(width: u32, height: u32, frames: u64, fps: f64) -> MediaFacts {
    MediaFacts { width, height, fps, frames, duration_sec: 0.0, video: frames > 1, container_unix: None }
}

fn pushed_request(lab: &Lab, media: MediaFacts) -> ScanRequest {
    let mut req = lab.request(lab.path("original.mov"));
    req.media = Some(media);
    req
}

/// Finish as the C API does: with the count the job was promised.
fn finish(job: Job, clean: bool) -> ScanReport {
    let declared = job.declared_frames;
    job.finish(DecodeEnd { clean, declared_frames: declared, reason: None }, &mut |_| {})
}

#[test]
fn a_phone_scan_of_pushed_frames_finds_the_marker_without_ffmpeg() {
    let lab = Lab::new();
    let scene = openworld_core::fixture::demo_scene().image;
    // The original is named for its age only. It is not decoded here.
    let req = pushed_request(&lab, facts(W, H, 1, 0.0));
    let mut job = open_pushed_job(&req, SystemTime::now()).expect("job");
    assert!(job.engine().wants(0));
    assert!(job.engine().push(0, &scene, &mut |_| {}));
    let report = finish(job, true);
    assert_candidate(&report, MISSING_POSTER);
    assert_eq!(report.counts.frames_declared, Some(1));
    assert_eq!(report.tracks.len(), 2, "{}", describe(&report));
    assert!(req.out.join("result.json").is_file());
}

#[test]
fn pushed_pages_that_are_not_a_video_each_decide_alone() {
    let lab = Lab::new();
    let at = spot(MISSING_ID, &[0]);
    // The person on the poster, then a stranger in the same place on the next page.
    let pages = [scene_at(MISSING_ID, at), scene_at(STRANGER_ID, at)];
    let req = pushed_request(&lab, MediaFacts { video: false, ..facts(W, H, 2, 1.0) });
    let mut job = open_pushed_job(&req, SystemTime::now()).unwrap();
    for (index, page) in pages.iter().enumerate() {
        assert!(job.engine().push(index as u64, page, &mut |_| {}));
    }
    let report = finish(job, true);
    // One page is enough to decide, and the stranger is not the same face.
    assert_candidate(&report, MISSING_POSTER);
    assert_eq!(report.counts.faces_seen, 2, "{}", describe(&report));
    assert!(report.tracks.iter().all(|t| t.first_frame == t.last_frame), "{}", describe(&report));
}

#[test]
fn a_phone_scan_of_a_jpeg_cut_short_is_incomplete_though_its_decoder_filled_the_gap() {
    let lab = Lab::new();
    let scene = openworld_core::fixture::demo_scene().image;
    let whole = lab.path("whole.jpg");
    scene.save(&whole).unwrap();
    let bytes = std::fs::read(&whole).unwrap();
    let mut req = lab.request(lab.write("cut.jpg", &bytes[..bytes.len() / 2]));
    req.media = Some(facts(W, H, 1, 0.0));
    let mut job = open_pushed_job(&req, SystemTime::now()).unwrap();
    // The platform decoder hands over a whole picture with gray where data was missing.
    assert!(job.engine().push(0, &scene, &mut |_| {}));
    let report = finish(job, true);
    assert_eq!(report.status, Status::Incomplete, "{}", describe(&report));
    assert_not_a_clearance(&report);
}

#[test]
fn a_phone_scan_with_no_class_on_is_refused_before_any_frame() {
    let lab = Lab::new();
    let mut req = pushed_request(&lab, facts(W, H, 1, 0.0));
    (req.missing, req.wanted) = (false, false);
    let refused = open_pushed_job(&req, SystemTime::now()).err().expect("refused");
    assert!(refused.headline.is(copy::CHOOSE_CLASS), "{}", describe(&refused));
    assert!(!req.out.exists());
}

#[test]
fn a_larger_later_picture_is_looked_at_for_its_own_size() {
    let lab = Lab::new();
    let small = blank(320, 240);
    let large = openworld_core::fixture::demo_scene().image;
    let mut job = open_pushed_job(&pushed_request(&lab, facts(320, 240, 2, 30.0)), SystemTime::now()).unwrap();
    assert!(job.engine().push(0, &small, &mut |_| {}));
    assert!(job.engine().push(1, &large, &mut |_| {}));
    let report = finish(job, true);
    assert_eq!(report.detection_long_side, large.width().max(large.height()), "{}", describe(&report));
}

#[test]
fn a_pushed_scan_whose_result_cannot_be_written_is_incomplete() {
    let lab = Lab::new();
    let req = pushed_request(&lab, facts(W, H, 1, 0.0));
    let mut job = open_pushed_job(&req, SystemTime::now()).unwrap();
    job.engine().push(0, &openworld_core::fixture::demo_scene().image, &mut |_| {});
    fs::create_dir(req.out.join("result.json")).unwrap();
    let report = finish(job, true);
    assert_incomplete(&report, copy::RESULT_UNWRITTEN);
    assert!(!report.candidates.is_empty(), "the candidate is still shown; the scan is not called finished");
}

#[test]
fn pushed_frames_must_arrive_in_order() {
    let lab = Lab::new();
    let mut job = open_pushed_job(&pushed_request(&lab, facts(W, H, 3, 30.0)), SystemTime::now()).unwrap();
    assert!(job.engine().push(0, &plain(0), &mut |_| {}));
    assert!(!job.engine().push(2, &plain(2), &mut |_| {}), "frame 1 was never pushed");
    assert_incomplete(&finish(job, true), copy::NOT_FULLY_DECODED);

    let mut job = open_pushed_job(&pushed_request(&lab, facts(W, H, 2, 30.0)), SystemTime::now()).unwrap();
    assert!(job.engine().push(0, &plain(0), &mut |_| {}));
    assert!(job.engine().push(1, &plain(1), &mut |_| {}));
    assert_incomplete(&finish(job, false), copy::NOT_FULLY_DECODED);
}

#[test]
fn a_phone_asks_before_converting_and_skips_what_measured_coverage_does_not_read() {
    let lab = Lab::new();
    let mut req = pushed_request(&lab, facts(W, H, 30, 30.0));
    req.coverage = Coverage::Measured;
    let mut job = open_pushed_job(&req, SystemTime::now()).unwrap();
    let mut read = Vec::new();
    for i in 0..30u64 {
        if job.engine().wants(i) {
            read.push(i);
            assert!(job.engine().push(i, &plain(i as u32), &mut |_| {}));
        } else {
            assert!(job.engine().skip(i));
        }
    }
    assert_eq!(read, [0, 6, 12, 18, 24]);
    let report = finish(job, true);
    assert_no_candidate(&report);
    assert_eq!((report.counts.frames_decoded, report.counts.frames_analyzed), (30, 5));
    assert!(report.coverage_banner.as_ref().is_some_and(|l| l.is(copy::BRIEF_FACE)));

    // Skipping a frame the coverage reads is not allowed.
    let mut job = open_pushed_job(&req, SystemTime::now()).unwrap();
    assert!(job.engine().wants(0));
    assert!(!job.engine().skip(0));
    assert_incomplete(&finish(job, true), copy::NOT_FULLY_DECODED);
}

#[test]
fn a_phone_request_is_checked_before_any_frame() {
    let lab = Lab::new();
    let cases: Vec<(Option<MediaFacts>, Copy)> = vec![
        (None, copy::R_BAD_REQUEST),
        (Some(facts(0, 10, 1, 0.0)), copy::R_BAD_CODEC),
        (Some(facts(20, 20, 0, 1.0)), copy::R_BAD_CODEC),
        (Some(facts(64, 64, 2, 0.0)), copy::R_NO_FPS),
        (Some(facts(64, 64, 2, f64::NAN)), copy::R_NO_FPS),
        (Some(facts(64, 64, 2, f64::INFINITY)), copy::R_NO_FPS),
        (Some(facts(16_000, 16_000, 1, 0.0)), copy::R_TOO_LARGE),
        // A count no file holds: planning it would list every frame number.
        (Some(facts(64, 64, 1_000_000_000_000, 30.0)), copy::R_TOO_LARGE),
    ];
    for (media, code) in cases {
        let mut req = lab.request(lab.path("clip.mov"));
        req.media = media.clone();
        let refused = open_pushed_job(&req, SystemTime::now()).err().unwrap_or_else(|| panic!("{media:?} opened"));
        assert_refused(&refused, code);
        assert!(!req.out.exists(), "{media:?}");
        if media.is_some() {
            assert!(estimate_for(&req).unwrap_err().headline.is(code), "estimate {media:?}");
        }
    }
}

#[test]
fn a_phone_scan_reads_the_container_clock() {
    let lab = Lab::new();
    let make = |name: &str, clock: Option<&str>| {
        let path = lab.path(name);
        let mut args = vec!["-f", "lavfi", "-i", "color=c=blue:s=64x64", "-frames:v", "1", "-r", "1", "-an", "-c:v", "libx264"];
        let tag = clock.map(|c| format!("creation_time={c}"));
        if let Some(tag) = &tag {
            args.extend(["-metadata", tag.as_str()]);
        }
        let mut args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
        args.push(path.clone().into());
        ffmpeg(args);
        path
    };
    let dated = make("dated.mp4", Some("2020-01-01T00:00:00Z"));
    let undated = make("plain.mp4", None);
    let phone = |input: &std::path::Path, container_unix: Option<i64>| {
        let mut req = lab.request(input);
        req.media = Some(MediaFacts { container_unix, ..facts(64, 64, 1, 1.0) });
        let mut job = open_pushed_job(&req, SystemTime::now()).unwrap();
        job.engine().push(0, &blank(64, 64), &mut |_| {});
        finish(job, true)
    };
    let warned = phone(&dated, None);
    assert_complete(&warned);
    assert_eq!(codes(&warned.warnings), ["timestamps_disagree"]);
    assert!(phone(&undated, None).warnings.is_empty());
    // The shell's own reading of the clock wins over the file's.
    let mtime = fs::metadata(&dated).unwrap().modified().unwrap();
    let secs = mtime.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;
    assert!(phone(&dated, Some(secs)).warnings.is_empty());
    // A clock at the ends of the range neither refuses nor stops the scan. One far
    // from the file time may warn that they disagree; it is never a clearance claim.
    for odd in [i64::MAX, i64::MIN] {
        let report = phone(&undated, Some(odd));
        assert_complete(&report);
        assert!(codes(&report.warnings).iter().all(|c| *c == "timestamps_disagree"), "{odd}: {:?}", report.warnings);
    }
}

#[test]
fn a_phone_estimate_warns_about_heat_battery_and_a_long_scan() {
    let lab = Lab::new();
    let phone = openworld_core::Device { kind: "phone".into(), ..Default::default() };
    let mut req = pushed_request(&lab, MediaFacts { duration_sec: 100.0, ..facts(1280, 720, 3000, 30.0) });
    req.device = phone.clone();
    let short = estimate_for(&req).unwrap();
    assert_eq!(short.frames_analyzed, 3000);
    assert!(short.timed, "demo markers time themselves");
    assert!(short.notes.iter().any(|n| n.is(copy::HEAT_NOTE)));
    assert!(!short.suggest_computer);

    // An hour and a half of video on a warm phone: suggest a computer, still allow complete.
    req.media = Some(MediaFacts { duration_sec: 6000.0, ..facts(1280, 720, 180_000, 30.0) });
    req.device = openworld_core::Device { thermal: Some("serious".into()), ..phone };
    let long = estimate_for(&req).unwrap();
    assert!(long.suggest_computer, "{} s", long.seconds);
    for note in [copy::HEAT_NOTE, copy::ALREADY_WARM, copy::BATTERY_NOTE, copy::SUGGEST_COMPUTER] {
        assert!(long.notes.iter().any(|n| n.is(note)), "{note:?} {:?}", long.notes);
    }
    assert_eq!(long.coverage, "complete");
    req.coverage = Coverage::Measured;
    let measured = estimate_for(&req).unwrap();
    assert_eq!(measured.frames_analyzed, 30_000);
    assert!(measured.seconds < long.seconds);

    // A computer gets no heat note.
    req.device = Default::default();
    assert!(!estimate_for(&req).unwrap().notes.iter().any(|n| n.is(copy::HEAT_NOTE)));
}

// ---------------------------------------------------------------------------
// The C interface the shells link.

fn answer(ptr: *mut std::ffi::c_char) -> Value {
    assert!(!ptr.is_null());
    let text = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
    ffi::ow_string_free(ptr);
    serde_json::from_str(&text).unwrap()
}

fn start(request: &Value) -> Result<*mut ffi::OwJob, Value> {
    let text = CString::new(request.to_string()).unwrap();
    let mut refusal = std::ptr::null_mut();
    let job = ffi::ow_job_start(text.as_ptr(), None, std::ptr::null_mut(), &mut refusal);
    if job.is_null() {
        Err(answer(refusal))
    } else {
        Ok(job)
    }
}

fn push_rgb(job: *mut ffi::OwJob, index: u64, frame: &RgbImage) -> i32 {
    let (w, h) = frame.dimensions();
    ffi::ow_job_push_pixels(job, index, frame.as_raw().as_ptr(), w, h, w * 3, ffi::OW_RGB, 0)
}

fn end(job: *mut ffi::OwJob) -> Value {
    let clean = CString::new(r#"{"clean":true}"#).unwrap();
    answer(ffi::ow_job_finish(job, clean.as_ptr()))
}

#[test]
fn the_c_interface_scans_pushed_frames_and_refuses_what_it_cannot_read() {
    let lab = Lab::new();
    // A turned frame: the shell says how far to turn it to show it upright.
    let (shown, _) = scene(MISSING_ID, &[0]);
    let stored = image::imageops::rotate270(&shown);
    let request = json!({"v": 1, "bundles": catalog(), "posters": lab.posters, "out": lab.out(), "media": {"width": W, "height": H, "frames": 1}});
    let job = start(&request).unwrap();
    assert_eq!(ffi::ow_job_wants(job, 0), 1);
    let (w, h) = stored.dimensions();
    assert_eq!(ffi::ow_job_push_pixels(job, 0, stored.as_raw().as_ptr(), w, h, w * 3, ffi::OW_RGB, 90), 1);
    let report = end(job);
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["status"], "complete");
    assert_eq!(report["headline"]["code"], "possible_candidate");
    assert_eq!(report["candidates"][0]["poster"]["id"], MISSING_POSTER);

    // Refusals come back as JSON with the code.
    let no_class = json!({"v": 1, "bundles": catalog(), "posters": lab.posters, "out": lab.out(), "missing": false, "wanted": false, "media": {"width": 8, "height": 8, "frames": 1}});
    assert_eq!(start(&no_class).unwrap_err()["refusal"], "choose_class");
    let no_pack = json!({"v": 1, "bundles": catalog(), "posters": lab.path("none"), "out": lab.out(), "media": {"width": 8, "height": 8, "frames": 1}});
    assert_eq!(start(&no_pack).unwrap_err()["refusal"], "missing_pack");
    let misspelled = json!({"v": 1, "bundles": catalog(), "posters": lab.posters, "out": lab.out(), "medai": {}});
    assert_eq!(start(&misspelled).unwrap_err()["refusal"], "bad_request");

    // Null pointers are refusals, not crashes.
    assert!(ffi::ow_job_start(std::ptr::null(), None, std::ptr::null_mut(), std::ptr::null_mut()).is_null());
    let called = answer(ffi::ow_call(std::ptr::null(), None, std::ptr::null_mut()));
    assert_eq!(called["ok"], false);
    assert_eq!(ffi::ow_job_wants(std::ptr::null_mut(), 0), 0);
    assert_eq!(push_rgb(std::ptr::null_mut(), 0, &blank(4, 4)), 0);
    assert!(ffi::ow_job_stop_token(std::ptr::null_mut()).is_null());
    ffi::ow_string_free(std::ptr::null_mut());
    ffi::ow_job_free(std::ptr::null_mut());
    let done = answer(ffi::ow_job_finish(std::ptr::null_mut(), std::ptr::null()));
    assert_eq!(done["ok"], false);

    // A typed call through the same entry.
    let copy_call = CString::new(r#"{"v":1,"op":"copy"}"#).unwrap();
    let copied = answer(ffi::ow_call(copy_call.as_ptr(), None, std::ptr::null_mut()));
    assert_eq!(copied["copy"]["no_clearance"], copy::NO_CLEARANCE.1);
}

#[test]
fn a_stop_token_ends_a_pushed_job_as_incomplete() {
    let lab = Lab::new();
    for (reason, code) in [("heat", "stopped_heat"), ("battery", "stopped_battery"), ("background", "stopped_background"), ("cancel", "cancelled")] {
        let request = json!({"v": 1, "bundles": catalog(), "posters": lab.posters, "out": lab.out(), "media": {"width": W, "height": H, "frames": 90, "fps": 30.0, "video": true}});
        let job = start(&request).unwrap();
        let token = ffi::ow_job_stop_token(job);
        assert_eq!(push_rgb(job, 0, &plain(0)), 1);
        let why = CString::new(reason).unwrap();
        ffi::ow_stop(token, why.as_ptr());
        assert_eq!(push_rgb(job, 1, &plain(1)), 0, "{reason}");
        ffi::ow_stop_free(token);
        let report = end(job);
        assert_eq!(report["status"], "incomplete", "{reason}");
        assert_eq!(report["reason"]["code"], code);
        assert_eq!(report["headline"]["code"], "incomplete");
        assert!(report["disclosure"].as_array().unwrap().iter().all(|l| l["code"] != "no_clearance"));
    }
}
