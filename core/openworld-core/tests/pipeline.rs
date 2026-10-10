// SPDX-License-Identifier: Apache-2.0

use image::{ImageEncoder, Rgb, RgbImage};
use openworld_core::bundle::load_bundles;
use openworld_core::copy::{
    BELOW_CUTOFF, BRIEF_FACE, FACE_UNSCORED, FIXTURE_MARKERS, INCOMPLETE, NO_CLEARANCE, NOT_COMPARED,
    PLATE_NOT_ON_POSTER, PLATE_UNPUBLISHED, PLATE_UNREAD, POSSIBLE_CANDIDATE, VEHICLE_NOT_PERSON,
};
use openworld_core::estimate::{Coverage, DetectionSize, FormFactor};
use openworld_core::fiducial::{self, render_face_module, render_plate, render_vehicle};
use openworld_core::geom::{blank, resize_long_side, Rect, FACE_COMPARE_PX, FACE_SEEN_PX};
use openworld_core::hardware::{execution_from_provider, Execution};
use openworld_core::posters::{self, sha256_hex};
use openworld_core::scan::{leave_prompt, media_warnings, scan_images, scan_path, MediaFacts, ScanOpts, ScanRequest};
use openworld_core::scene::demo_scene;
use openworld_core::timeutil::parse_rfc3339;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bundles() -> Vec<openworld_core::Bundle> {
    load_bundles(&repo().join("bundles")).expect("bundles")
}

fn fast() -> openworld_core::Bundle {
    bundles().into_iter().find(|b| b.id == "fast").unwrap()
}

fn now() -> SystemTime {
    parse_rfc3339("2026-10-07T00:00:00Z").unwrap()
}

fn opts(detection: DetectionSize, coverage: Coverage) -> ScanOpts {
    ScanOpts {
        detection,
        coverage,
        execution: Execution::Cpu,
        form_factor: FormFactor::Computer,
        missing: true,
        wanted: true,
        abort_after_frames: None,
        fps: 30.0,
        frame_count_hint: None,
        warnings: Vec::new(),
        out_dir: None,
    }
}

fn paint_face(id: u16, module: u32, x: u32, y: u32, w: u32, h: u32) -> RgbImage {
    let mut canvas = blank(w, h);
    fiducial::place(&mut canvas, &render_face_module(id, module), x, y);
    canvas
}

#[test]
fn catalog_rows_name_fast_and_keep_accurate_unmeasured() {
    let all = bundles();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, "fast");
    let fast = all.iter().find(|b| b.id == "fast").unwrap();
    let accurate = all.iter().find(|b| b.id == "accurate").unwrap();
    assert!(fast.preselected());
    assert!(!accurate.preselected());
    assert_eq!(fast.best_for, "Phones and long video.");
    assert_eq!(accurate.best_for, "A computer, when you want fewer misses.");
    assert!(!accurate.best_for.to_ascii_lowercase().contains("more accurate"));
    assert_eq!(fast.threshold, 0.55);
    assert_eq!(accurate.threshold, 0.60);
    assert_eq!(fast.detector, "SCRFD-0.5GF");
    assert_eq!(accurate.detector, "SCRFD-10GF");
    assert_eq!(accurate.embedder, "ArcFace-R100");
    assert!(!fast.weights_ready);
    assert!(!accurate.weights_ready);
    assert!(!accurate.curve_exists);
    assert_eq!(accurate.curve_line, "Not measured yet.");
    assert!(!accurate.real_posters_allowed);
    if fast.curve_exists {
        assert!(!fast.real_posters_allowed);
    }
}

#[test]
fn unofficial_manifest_must_say_so() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("dev");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(
        bundle.join("manifest.toml"),
        r#"
schema = "openworld.bundle.v1"
id = "dev"
name = "Dev"
version = "0.0.1"
official = false
best_for = "A local experiment."
threshold = 0.5
estimate_factor = 1.0
[models]
detector = "custom"
detector_version = "0"
embedder = "custom"
embedder_version = "0"
plate = "RTMDet-nano"
plate_version = "0"
plate_license = "Apache-2.0"
"#,
    )
    .unwrap();
    assert!(load_bundles(dir.path()).is_err());
}

#[test]
fn demo_scan_shows_the_face_strip_and_the_disclosure() {
    let bundle = fast();
    let dir = tempfile::tempdir().unwrap();
    let pack_dir = dir.path().join("posters");
    posters::write_fixture_pack(&pack_dir, now()).unwrap();
    let pack = posters::load_pack(&pack_dir, now()).unwrap();
    let scene = demo_scene(bundle.threshold);
    let mut opts = opts(DetectionSize::Px(640), Coverage::Complete);
    opts.fps = 0.0;
    opts.out_dir = Some(dir.path().join("out"));
    let report = scan_images(&[scene.image], &bundle, &pack, &opts, &mut |_| {});
    assert_eq!(report.status, "complete");
    assert_eq!(report.bundle_name, "Fast");
    assert_eq!(report.class_note.as_deref(), Some("Missing and wanted."));
    assert!(report.disclosure.iter().any(|l| l == "Nothing is uploaded."));
    assert!(report.disclosure.iter().any(|l| l == "Nobody is enrolled."));
    assert!(report.disclosure.iter().any(|l| l.contains("does not train")));
    assert!(report.disclosure.iter().any(|l| l.contains("does not contact")));
    assert!(report.disclosure.iter().any(|l| l == NO_CLEARANCE));
    assert!(report.disclosure.iter().any(|l| l.contains("not authenticated")));
    let labels: Vec<_> = report.inventory.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&NOT_COMPARED), "{labels:?}");
    assert!(labels.contains(&VEHICLE_NOT_PERSON), "{labels:?}");
    assert!(report.candidates.iter().any(|c| c.wording == POSSIBLE_CANDIDATE && c.kind == "face"));
    assert!(report.candidates.iter().any(|c| c.kind == "plate" && c.plate.as_deref() == Some("FIX123")));
    assert!(report.plates_ocr_attempted >= 1);
    assert!(report.faces_seen_not_compared >= 1);
    assert!(report.faces_embedded >= 1);
    assert!(report.inventory.iter().any(|i| i.crop.is_some()));
    assert_eq!(report.perception_note, FIXTURE_MARKERS);
    assert!(!report.perception_note.contains("SCRFD"));
    let face = report.candidates.iter().find(|c| c.kind == "face").unwrap();
    assert!(face.uncertainty.contains("Score "));
    assert!(face.uncertainty.contains("Fast keeps a candidate at"));
    assert!(!face.uncertainty.contains("Cosine"));
    let plate = report.candidates.iter().find(|c| c.kind == "plate").unwrap();
    assert!(plate.uncertainty.contains("FIX123"));
    assert_eq!(face.leaving, "You are leaving OpenWorld.");
    assert!(face.cosine.unwrap() >= bundle.threshold);
}

#[test]
fn gates_64_and_112_and_full_res_versus_preview() {
    let bundle = fast();
    let pack_dir = tempfile::tempdir().unwrap();
    posters::write_fixture_pack(pack_dir.path(), now()).unwrap();
    let pack = posters::load_pack(pack_dir.path(), now()).unwrap();

    let tiny = paint_face(7, 4, 20, 20, 200, 160);
    let tiny_report = scan_images(&[tiny], &bundle, &pack, &opts(DetectionSize::Full, Coverage::Complete), &mut |_| {});
    assert!(tiny_report.inventory.iter().all(|i| i.kind != "face"));
    assert!(tiny_report.candidates.is_empty());
    assert_eq!(tiny_report.summary, NO_CLEARANCE);

    let seen = paint_face(7, 8, 20, 20, 240, 180);
    assert_eq!(render_face_module(7, 8).width(), FACE_SEEN_PX);
    let seen_report = scan_images(&[seen], &bundle, &pack, &opts(DetectionSize::Full, Coverage::Complete), &mut |_| {});
    let face = seen_report.inventory.iter().find(|i| i.kind == "face").unwrap();
    assert_eq!(face.label, NOT_COMPARED);
    assert!(face.orig_short_px < FACE_COMPARE_PX);
    assert_eq!(seen_report.faces_embedded, 0);
    assert!(seen_report.candidates.iter().all(|c| c.kind != "face"));

    let mut wide = blank(3840, 2160);
    fiducial::place(&mut wide, &render_face_module(7, 48), 600, 300);
    let (preview, map) = resize_long_side(&wide, 640);
    assert_eq!(preview.dimensions(), (640, 360));
    let orig = map.to_original(Rect { x: 100, y: 50, w: 64, h: 64 });
    assert_eq!((orig.x, orig.y, orig.w), (600, 300, 384));
    let wide_report = scan_images(&[wide], &bundle, &pack, &opts(DetectionSize::Px(640), Coverage::Complete), &mut |_| {});
    let compared = wide_report.inventory.iter().find(|i| i.kind == "face").expect("preview should see the face");
    assert!(compared.compared, "{compared:?}");
    assert!(compared.orig_short_px >= FACE_COMPARE_PX);
    assert!(wide_report.candidates.iter().any(|c| c.kind == "face"));
}

#[test]
fn plate_is_not_read_without_a_published_plate_and_a_vehicle_is_not_a_person() {
    let bundle = fast();
    let dir = tempfile::tempdir().unwrap();
    posters::write_fixture_pack(dir.path(), now()).unwrap();
    let mut pack = posters::load_pack(dir.path(), now()).unwrap();
    for poster in &mut pack.posters {
        poster.plate = None;
    }
    let mut image = blank(400, 240);
    fiducial::place(&mut image, &render_plate("FIX123", 8).unwrap(), 16, 16);
    fiducial::place(&mut image, &render_vehicle(8), 220, 40);
    let report = scan_images(&[image], &bundle, &pack, &opts(DetectionSize::Full, Coverage::Complete), &mut |_| {});
    assert_eq!(report.plates_ocr_attempted, 0);
    assert!(report.candidates.is_empty());
    assert!(report.inventory.iter().any(|i| i.kind == "vehicle" && i.label == VEHICLE_NOT_PERSON));
    assert!(report.inventory.iter().all(|i| i.kind != "face"));
    let plate = report.inventory.iter().find(|i| i.kind == "plate").unwrap();
    assert_eq!(plate.label, PLATE_UNPUBLISHED);
    assert!(!plate.compared);
    assert_ne!(plate.label, NOT_COMPARED);
    assert_ne!(plate.label, PLATE_NOT_ON_POSTER);
    assert_ne!(plate.label, PLATE_UNREAD);
    let _ = pack;
}

#[test]
fn a_large_face_that_cannot_be_scored_is_not_a_clearance_or_a_cutoff() {
    let bundle = fast();
    let dir = tempfile::tempdir().unwrap();
    posters::write_fixture_pack(dir.path(), now()).unwrap();
    let pack = posters::load_pack(dir.path(), now()).unwrap();
    let mut image = blank(640, 426);
    fiducial::place(&mut image, &render_face_module(7, 24), 1, 0);
    let report = scan_images(&[image], &bundle, &pack, &opts(DetectionSize::Px(320), Coverage::Complete), &mut |_| {});
    assert!(report.candidates.is_empty());
    assert_eq!(report.summary, NO_CLEARANCE);
    assert_eq!(report.faces_embedded, 0);
    let row = report.inventory.iter().find(|item| item.kind == "face").unwrap();
    assert!(row.orig_short_px >= FACE_COMPARE_PX);
    assert_eq!(row.label, FACE_UNSCORED);
    assert!(!row.compared);
    assert_ne!(row.label, NOT_COMPARED);
    assert_ne!(row.label, BELOW_CUTOFF);
    assert_ne!(row.label, "No score.");
}

#[test]
fn a_plate_that_does_not_match_is_not_below_a_cutoff() {
    let bundle = fast();
    let dir = tempfile::tempdir().unwrap();
    posters::write_fixture_pack(dir.path(), now()).unwrap();
    let pack = posters::load_pack(dir.path(), now()).unwrap();
    let mut other = blank(400, 240);
    fiducial::place(&mut other, &render_plate("OTHER1", 8).unwrap(), 16, 16);
    let mismatch = scan_images(&[other], &bundle, &pack, &opts(DetectionSize::Full, Coverage::Complete), &mut |_| {});
    assert!(mismatch.candidates.is_empty());
    assert_eq!(mismatch.summary, NO_CLEARANCE);
    assert!(mismatch.plates_ocr_attempted >= 1);
    let row = mismatch.inventory.iter().find(|item| item.kind == "plate").unwrap();
    assert_eq!(row.label, PLATE_NOT_ON_POSTER);
    assert!(row.compared);
    assert_ne!(row.label, "Below the locked cutoff. Not a candidate.");

    let mut broken = render_plate("FIX123", 8).unwrap();
    let pixel = broken.get_pixel(20, 20).0;
    let flipped = if pixel[0] == 0 { 255 } else { 0 };
    broken.put_pixel(20, 20, Rgb([flipped, flipped, flipped]));
    let mut unread = blank(400, 240);
    fiducial::place(&mut unread, &broken, 16, 16);
    let failed = scan_images(&[unread], &bundle, &pack, &opts(DetectionSize::Full, Coverage::Complete), &mut |_| {});
    assert!(failed.candidates.is_empty());
    let row = failed.inventory.iter().find(|item| item.kind == "plate").unwrap();
    assert_eq!(row.label, PLATE_UNREAD);
    assert!(!row.compared);
}

#[test]
fn wanted_class_off_skips_that_class_with_the_same_cutoff() {
    let bundle = fast();
    let dir = tempfile::tempdir().unwrap();
    posters::write_fixture_pack(dir.path(), now()).unwrap();
    let pack = posters::load_pack(dir.path(), now()).unwrap();
    let scene = demo_scene(bundle.threshold);
    let image = scene.image;
    let mut opts = opts(DetectionSize::Px(640), Coverage::Complete);
    opts.wanted = false;
    opts.fps = 0.0;
    let report = scan_images(&[image.clone()], &bundle, &pack, &opts, &mut |_| {});
    assert!(report.candidates.iter().all(|c| c.poster_class == "missing"));
    assert!(report.candidates.iter().all(|c| c.kind != "plate"));
    assert_eq!(report.plates_ocr_attempted, 0);
    assert_eq!(report.class_note.as_deref(), Some("Missing."));
    assert_eq!(report.threshold, bundle.threshold);
    opts.missing = false;
    opts.wanted = true;
    let wanted_only = scan_images(&[image.clone()], &bundle, &pack, &opts, &mut |_| {});
    assert!(wanted_only.candidates.iter().all(|c| c.poster_class == "wanted"));
    assert!(wanted_only.candidates.iter().any(|c| c.kind == "plate"));
    assert_eq!(wanted_only.class_note.as_deref(), Some("Wanted."));
    opts.wanted = false;
    let neither = scan_images(&[image], &bundle, &pack, &opts, &mut |_| {});
    assert!(neither.candidates.is_empty());
    assert_eq!(neither.summary, NO_CLEARANCE);
    assert_eq!(neither.class_note.as_deref(), Some("No class was on."));
}

#[test]
fn measured_banner_and_a_stopped_job_is_incomplete() {
    let bundle = fast();
    let dir = tempfile::tempdir().unwrap();
    posters::write_fixture_pack(dir.path(), now()).unwrap();
    let pack = posters::load_pack(dir.path(), now()).unwrap();
    let frame = paint_face(7, 16, 16, 16, 320, 240);
    let frames = vec![frame; 12];
    let mut measured = opts(DetectionSize::Full, Coverage::Measured);
    let report = scan_images(&frames, &bundle, &pack, &measured, &mut |_| {});
    assert_eq!(report.coverage_banner.as_deref(), Some(BRIEF_FACE));
    assert!(report.frames_analyzed < report.frames_decoded);
    assert_eq!(report.status, "complete");

    measured.coverage = Coverage::Complete;
    measured.abort_after_frames = Some(2);
    let stopped = scan_images(&frames, &bundle, &pack, &measured, &mut |_| {});
    assert_eq!(stopped.status, "incomplete");
    assert_eq!(stopped.summary, INCOMPLETE);
    assert_ne!(stopped.summary, NO_CLEARANCE);
    assert!(stopped.disclosure.iter().all(|line| line != NO_CLEARANCE));
    assert!(stopped.disclosure.iter().any(|line| line.contains("Nothing is uploaded.")));
    assert!(stopped.frames_analyzed < frames.len() as u64);
}

#[test]
fn bad_hash_expired_pack_and_missing_weights_refuse() {
    let dir = tempfile::tempdir().unwrap();
    posters::write_fixture_pack(dir.path(), now()).unwrap();
    let mut bytes = fs::read(dir.path().join("snapshot.json")).unwrap();
    bytes[0] = b' ';
    fs::write(dir.path().join("snapshot.json"), &bytes).unwrap();
    let bundle = fast();
    let image = paint_face(7, 16, 16, 16, 320, 240);
    let opts = opts(DetectionSize::Full, Coverage::Complete);
    let missing = tempfile::tempdir().unwrap();
    let refused = scan_path(
        &ScanRequest {
            input: missing.path().join("nope.png"),
            bundles_dir: repo().join("bundles"),
            bundle_id: "fast".into(),
            posters_dir: dir.path().to_path_buf(),
            out_dir: missing.path().join("out"),
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Computer,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(refused.status, "refused");
    assert!(refused.refusal == Some("bad_codec".into()) || refused.refusal == Some("bad_hash".into()));

    image.save(dir.path().join("in.png")).unwrap();
    let hashed = scan_path(
        &ScanRequest {
            input: dir.path().join("in.png"),
            bundles_dir: repo().join("bundles"),
            bundle_id: bundle.id.clone(),
            posters_dir: dir.path().to_path_buf(),
            out_dir: dir.path().join("out"),
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Phone,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(hashed.refusal.as_deref(), Some("bad_hash"));
    assert_ne!(hashed.summary, NO_CLEARANCE);

    let expired = tempfile::tempdir().unwrap();
    let file_bytes = fs::read(repo().join("bundles/fast/manifest.toml")).unwrap();
    let _ = file_bytes;
    let pack_src = posters::load_pack(&{
        let fresh = tempfile::tempdir().unwrap();
        posters::write_fixture_pack(fresh.path(), now()).unwrap();
        // Rebuild an expired snapshot from the fresh bytes.
        let raw = fs::read(fresh.path().join("snapshot.json")).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        value["expires_at"] = serde_json::Value::String("2020-01-01T00:00:00Z".into());
        let pretty = serde_json::to_vec_pretty(&value).unwrap();
        fs::write(expired.path().join("snapshot.json"), &pretty).unwrap();
        fs::write(expired.path().join("snapshot.sha256"), format!("{}\n", sha256_hex(&pretty))).unwrap();
        fresh.into_path()
    }, now());
    let _ = pack_src;
    let expired_report = scan_path(
        &ScanRequest {
            input: dir.path().join("in.png"),
            bundles_dir: repo().join("bundles"),
            bundle_id: "fast".into(),
            posters_dir: expired.path().to_path_buf(),
            out_dir: expired.path().join("out"),
            detection: DetectionSize::Full,
            coverage: Coverage::Complete,
            form_factor: FormFactor::Computer,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(expired_report.refusal.as_deref(), Some("expired_pack"));

    let onnx = posters::load_pack(&{
        let fresh = tempfile::tempdir().unwrap();
        posters::write_fixture_pack(fresh.path(), now()).unwrap();
        fresh.into_path()
    }, now())
    .unwrap_or_else(|_| panic!("pack"));
    // The in-memory pack is fiducial. A file pack with perception onnx is refused.
    let onnx_dir = tempfile::tempdir().unwrap();
    let raw = fs::read(dir.path().join("snapshot.json"));
    let _ = (raw, onnx, opts);
    let fresh = tempfile::tempdir().unwrap();
    posters::write_fixture_pack(fresh.path(), now()).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(fresh.path().join("snapshot.json")).unwrap()).unwrap();
    value["perception"] = serde_json::Value::String("onnx".into());
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(onnx_dir.path().join("snapshot.json"), &pretty).unwrap();
    fs::write(onnx_dir.path().join("snapshot.sha256"), format!("{}\n", sha256_hex(&pretty))).unwrap();
    let weights = scan_path(
        &ScanRequest {
            input: dir.path().join("in.png"),
            bundles_dir: repo().join("bundles"),
            bundle_id: "fast".into(),
            posters_dir: onnx_dir.path().to_path_buf(),
            out_dir: onnx_dir.path().join("out"),
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Computer,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(weights.refusal.as_deref(), Some("missing_weights"));
}

#[test]
fn old_files_warn_and_the_leave_prompt_is_fbi_only() {
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(40 * 24 * 3600);
    let warnings = media_warnings(Some(old), Some(now), now);
    assert!(warnings.iter().any(|w| w.contains("30 days")));
    assert!(warnings.iter().any(|w| w.contains("timestamps disagree")));
    let recent = media_warnings(Some(now), None, now);
    assert!(recent.is_empty());
    let leave = leave_prompt("https://www.fbi.gov/wanted").unwrap();
    assert_eq!(leave.message, "You are leaving OpenWorld.");
    assert!(leave_prompt("https://example.com/").is_err());
    assert_eq!(execution_from_provider("CPUExecutionProvider"), Execution::Cpu);
}

#[test]
fn ffmpeg_decodes_a_lossless_fixture_video() {
    let dir = tempfile::tempdir().unwrap();
    let frames = dir.path().join("frames");
    fs::create_dir_all(&frames).unwrap();
    for i in 0..6 {
        let mut image = blank(200, 160);
        if i == 0 || i == 3 {
            fiducial::place(&mut image, &render_face_module(7, 16), 20, 16);
        }
        image.save(frames.join(format!("f{i:02}.png"))).unwrap();
    }
    let video = dir.path().join("clip.mkv");
    let status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-framerate", "30", "-i"])
        .arg(frames.join("f%02d.png"))
        .args(["-an", "-c:v", "ffv1"])
        .arg(&video)
        .status()
        .expect("ffmpeg");
    assert!(status.success());
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let out = dir.path().join("out");
    let report = scan_path(
        &ScanRequest {
            input: video,
            bundles_dir: repo().join("bundles"),
            bundle_id: "fast".into(),
            posters_dir: pack,
            out_dir: out,
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Computer,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(report.status, "complete", "{}", report.message);
    assert_eq!(report.frames_decoded, 6);
    assert_eq!(report.frames_analyzed, 6);
    assert!(report.inventory.iter().any(|i| i.kind == "face"));
    assert!(report.coverage_banner.is_none());
}

#[test]
fn a_one_frame_gif_is_not_repeated_into_extra_frames() {
    let dir = tempfile::tempdir().unwrap();
    let still = dir.path().join("blue.png");
    blank(64, 64).save(&still).unwrap();
    let gif = dir.path().join("blue.gif");
    let status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(&still)
        .args(["-frames:v", "1"])
        .arg(&gif)
        .status()
        .expect("ffmpeg");
    assert!(status.success());
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let report = scan_path(
        &ScanRequest {
            input: gif,
            bundles_dir: repo().join("bundles"),
            bundle_id: "fast".into(),
            posters_dir: pack,
            out_dir: dir.path().join("out"),
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Computer,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(report.status, "complete", "{}", report.message);
    assert_eq!(report.summary, NO_CLEARANCE);
    assert_eq!(report.frames_decoded, 1);
    assert!(report.candidates.is_empty());
}

#[test]
fn an_animated_png_keeps_a_face_that_is_not_on_the_first_frame() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    let (width, height) = scene.image.dimensions();
    let mut first = blank(width, height);
    first.put_pixel(0, 0, image::Rgb([0, 0, 0]));
    let mut third = blank(width, height);
    third.put_pixel(20, 20, image::Rgb([0, 0, 0]));
    first.save(dir.path().join("f0.png")).unwrap();
    scene.image.save(dir.path().join("f1.png")).unwrap();
    third.save(dir.path().join("f2.png")).unwrap();
    let apng = dir.path().join("later.apng");
    let status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-framerate", "5", "-start_number", "0", "-i"])
        .arg(dir.path().join("f%d.png"))
        .args(["-frames:v", "3", "-plays", "1", "-f", "apng"])
        .arg(&apng)
        .status()
        .expect("ffmpeg");
    assert!(status.success());
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let report = scan_path(
        &ScanRequest {
            input: apng,
            bundles_dir: repo().join("bundles"),
            bundle_id: "fast".into(),
            posters_dir: pack,
            out_dir: dir.path().join("out"),
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Computer,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(report.status, "complete", "{}", report.message);
    assert_eq!(report.summary, POSSIBLE_CANDIDATE);
    assert_eq!(report.frames_decoded, 3);
    assert!(!report.candidates.is_empty());
    assert!(report.candidates.iter().all(|candidate| candidate.frame_index == 1));
}

#[test]
fn platform_frames_scan_without_ffmpeg_and_media_facts_skip_probe() {
    let dir = tempfile::tempdir().unwrap();
    let frames = dir.path().join("frames");
    fs::create_dir_all(&frames).unwrap();
    let scene = demo_scene(fast().threshold);
    scene.image.save(frames.join("frame_000000.png")).unwrap();
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let report = scan_path(
        &ScanRequest {
            input: dir.path().join("original-that-is-not-decoded.png"),
            bundles_dir: repo().join("bundles"),
            bundle_id: "fast".into(),
            posters_dir: pack,
            out_dir: dir.path().join("out"),
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Phone,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: Some(frames.clone()),
            media: Some(MediaFacts {
                width: scene.image.width(),
                height: scene.image.height(),
                fps: 0.0,
                frames: 1,
                duration_sec: 0.0,
                video: false,
                container_unix: None,
            }),
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(report.status, "complete", "{}", report.message);
    assert!(report.summary.contains("Possible candidate"));
    assert!(report.inventory.iter().any(|item| item.label == NOT_COMPARED));
    assert!(report.disclosure.iter().any(|line| line.contains("Nothing is uploaded")));

    let bare = dir.path().join("two");
    fs::create_dir_all(&bare).unwrap();
    scene.image.save(bare.join("frame_000000.png")).unwrap();
    scene.image.save(bare.join("frame_000001.png")).unwrap();
    let refused = scan_path(
        &ScanRequest {
            input: dir.path().join("original-that-is-not-decoded.png"),
            bundles_dir: repo().join("bundles"),
            bundle_id: "fast".into(),
            posters_dir: dir.path().join("posters"),
            out_dir: dir.path().join("out-two"),
            detection: DetectionSize::Px(640),
            coverage: Coverage::Measured,
            form_factor: FormFactor::Computer,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: Some(bare),
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(refused.refusal.as_deref(), Some("bad_codec"));

    let est = openworld_core::scan::estimate_for(&ScanRequest {
        input: dir.path().join("missing.mov"),
        bundles_dir: repo().join("bundles"),
        bundle_id: "fast".into(),
        posters_dir: dir.path().join("posters"),
        out_dir: dir.path().join("unused"),
        detection: DetectionSize::Px(640),
        coverage: Coverage::Complete,
        form_factor: FormFactor::Phone,
        execution: Execution::Cpu,
        missing: true,
        wanted: true,
        abort_after_frames: None,
        frames_dir: None,
        media: Some(MediaFacts {
            width: 1280,
            height: 720,
            fps: 30.0,
            frames: 3000,
            duration_sec: 100.0,
            video: true,
            container_unix: None,
        }),
        now: now(),
    })
    .expect("platform facts");
    assert_eq!(est.frames_analyzed, 3000);
    assert!(est.heat_note.is_some());
    assert!(est.battery_note.is_some());
}

#[test]
fn a_pinned_onnx_bundle_that_does_not_load_refuses_instead_of_clearing() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_dir = dir.path().join("bundles/custom");
    std::fs::create_dir_all(bundle_dir.join("weights")).unwrap();
    let bytes = b"this is not an onnx model";
    let sha = sha256_hex(bytes);
    for name in ["det.onnx", "embed.onnx", "plate.onnx"] {
        std::fs::write(bundle_dir.join("weights").join(name), bytes).unwrap();
    }
    let manifest = format!(
        r#"
schema = "openworld.bundle.v1"
id = "custom"
name = "Custom"
version = "0.0.0"
official = true
best_for = "A computer, when the weights are real."
threshold = 0.55
estimate_factor = 1.0

[models]
detector = "SCRFD-0.5GF"
detector_version = "pinned-test"
embedder = "ArcFace-MBF"
embedder_version = "pinned-test"
plate = "RTMDet-nano"
plate_version = "pinned-test"
plate_license = "Apache-2.0"

[[files]]
role = "detector"
name = "SCRFD-0.5GF"
version = "pinned-test"
sha256 = "{sha}"
license = "Apache-2.0"
path = "weights/det.onnx"

[[files]]
role = "embedder"
name = "ArcFace-MBF"
version = "pinned-test"
sha256 = "{sha}"
license = "Apache-2.0"
path = "weights/embed.onnx"

[[files]]
role = "plate"
name = "RTMDet-nano"
version = "pinned-test"
sha256 = "{sha}"
license = "Apache-2.0"
path = "weights/plate.onnx"
"#
    );
    std::fs::write(bundle_dir.join("manifest.toml"), manifest).unwrap();
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(pack.join("snapshot.json")).unwrap()).unwrap();
    value["perception"] = serde_json::Value::String("onnx".into());
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(pack.join("snapshot.json"), &pretty).unwrap();
    fs::write(pack.join("snapshot.sha256"), format!("{}\n", sha256_hex(&pretty))).unwrap();
    let image = paint_face(7, 16, 16, 16, 320, 240);
    image.save(dir.path().join("in.png")).unwrap();
    let report = scan_path(
        &ScanRequest {
            input: dir.path().join("in.png"),
            bundles_dir: dir.path().join("bundles"),
            bundle_id: "custom".into(),
            posters_dir: pack,
            out_dir: dir.path().join("out"),
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Computer,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(report.status, "refused", "{}", report.message);
    assert_ne!(report.summary, NO_CLEARANCE);
    assert!(report.message.contains("Refusing"));
}


#[test]
fn a_pinned_onnx_session_that_runs_can_finish_clear() {
    // Constant zeros. Apache graphs so the session path can run. These are not SCRFD or ArcFace weights.
    let dir = tempfile::tempdir().unwrap();
    let bundle_dir = dir.path().join("bundles/custom");
    std::fs::create_dir_all(bundle_dir.join("weights")).unwrap();
    let models = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/models");
    let mut pins = Vec::new();
    for (file, role, model_name) in [
        ("detector.onnx", "detector", "SCRFD-0.5GF"),
        ("embedder.onnx", "embedder", "ArcFace-MBF"),
        ("plate.onnx", "plate", "RTMDet-nano"),
    ] {
        let bytes = fs::read(models.join(file)).unwrap();
        let sha = sha256_hex(&bytes);
        fs::write(bundle_dir.join("weights").join(file), &bytes).unwrap();
        pins.push((role, model_name, sha, file));
    }
    let mut manifest = r#"
schema = "openworld.bundle.v1"
id = "custom"
name = "Custom"
version = "0.0.0"
official = true
best_for = "A computer, when the weights are real."
threshold = 0.55
estimate_factor = 1.0

[models]
detector = "SCRFD-0.5GF"
detector_version = "pinned-test"
embedder = "ArcFace-MBF"
embedder_version = "pinned-test"
plate = "RTMDet-nano"
plate_version = "pinned-test"
plate_license = "Apache-2.0"
"#
    .to_string();
    for (role, model_name, sha, file) in &pins {
        manifest.push_str(&format!(
            r#"
[[files]]
role = "{role}"
name = "{model_name}"
version = "pinned-test"
sha256 = "{sha}"
license = "Apache-2.0"
path = "weights/{file}"
"#
        ));
    }
    fs::write(bundle_dir.join("manifest.toml"), manifest).unwrap();
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(pack.join("snapshot.json")).unwrap()).unwrap();
    value["perception"] = serde_json::Value::String("onnx".into());
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(pack.join("snapshot.json"), &pretty).unwrap();
    fs::write(pack.join("snapshot.sha256"), format!("{}\n", sha256_hex(&pretty))).unwrap();
    let image = paint_face(7, 16, 16, 16, 320, 240);
    image.save(dir.path().join("in.png")).unwrap();
    let report = scan_path(
        &ScanRequest {
            input: dir.path().join("in.png"),
            bundles_dir: dir.path().join("bundles"),
            bundle_id: "custom".into(),
            posters_dir: pack,
            out_dir: dir.path().join("out"),
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Computer,
            execution: Execution::Gpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert_eq!(report.status, "complete", "{}", report.message);
    assert_eq!(report.summary, NO_CLEARANCE);
    assert_eq!(report.perception, "onnx");
    assert_eq!(report.perception_note, "Weights from Custom ran.");
    assert_eq!(report.faces_embedded, 0);
    assert!(report.inventory.is_empty());
    assert!(report.candidates.is_empty());
    // The request said GPU. The session loaded CPU, so the note follows the session.
    assert_eq!(report.execution, "cpu");
    assert!(report.device_note.unwrap().contains("CPU"));
}

fn jpeg_with_orientation(image: &RgbImage, orientation: u8) -> Vec<u8> {
    let mut encoded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 100)
        .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
        .unwrap();
    let mut tiff = Vec::new();
    tiff.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00]);
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&(u16::from(orientation)).to_le_bytes());
    tiff.extend_from_slice(&0u16.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&tiff);
    let len = (payload.len() + 2) as u16;
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&encoded[2..]);
    out
}

fn webp_with_orientation(image: &RgbImage, orientation: u8) -> Vec<u8> {
    let mut encoded = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
        .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
        .unwrap();
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
    let mut tiff = Vec::new();
    tiff.extend_from_slice(&[0x4D, 0x4D, 0x00, 0x2A]);
    tiff.extend_from_slice(&8u32.to_be_bytes());
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&0x0112u16.to_be_bytes());
    tiff.extend_from_slice(&3u16.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&u16::from(orientation).to_be_bytes());
    tiff.extend_from_slice(&0u16.to_be_bytes());
    let width = image.width() - 1;
    let height = image.height() - 1;
    let vp8x = vec![
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

fn png_with_orientation(image: &RgbImage, orientation: u8) -> Vec<u8> {
    let mut encoded = Vec::new();
    image::codecs::png::PngEncoder::new(&mut encoded)
        .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
        .unwrap();
    let mut tiff = Vec::new();
    tiff.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00]);
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&(u16::from(orientation)).to_le_bytes());
    tiff.extend_from_slice(&0u16.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    let mut typed = b"eXIf".to_vec();
    typed.extend_from_slice(&tiff);
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in &typed {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    let crc = !crc;
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&(tiff.len() as u32).to_be_bytes());
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
fn a_png_with_camera_orientation_is_scanned_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    let stored = image::imageops::rotate270(&scene.image);
    let side = dir.path().join("side.png");
    fs::write(&side, png_with_orientation(&stored, 6)).unwrap();
    let raw = dir.path().join("raw.png");
    let mut raw_bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut raw_bytes)
        .write_image(stored.as_raw(), stored.width(), stored.height(), image::ExtendedColorType::Rgb8)
        .unwrap();
    fs::write(&raw, raw_bytes).unwrap();
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf| {
        let out_dir = dir.path().join(format!("out-{}", input.file_name().unwrap().to_string_lossy()));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Complete,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let probed = openworld_core::decode::probe(&side).unwrap();
    assert_eq!((probed.width, probed.height), scene.image.dimensions());
    let turned = scan(side);
    assert_eq!(turned.status, "complete", "{}", turned.message);
    assert_eq!(turned.summary, POSSIBLE_CANDIDATE);
    assert!(!turned.candidates.is_empty());
    let sideways = scan(raw);
    assert_eq!(sideways.status, "complete", "{}", sideways.message);
    assert_eq!(sideways.summary, NO_CLEARANCE);
    assert!(sideways.candidates.is_empty());
}

fn tiff_entry(tag: u16, kind: u16, count: u32, value: u32) -> [u8; 12] {
    let mut out = [0u8; 12];
    out[0..2].copy_from_slice(&tag.to_le_bytes());
    out[2..4].copy_from_slice(&kind.to_le_bytes());
    out[4..8].copy_from_slice(&count.to_le_bytes());
    out[8..12].copy_from_slice(&value.to_le_bytes());
    out
}

/// Uncompressed RGB pages. `orientation` is the camera tag for that page.
fn rgb_tiff(pages: &[(u32, u32, &[u8], u16)]) -> Vec<u8> {
    let entry_count = 10u16;
    let ifd_len = 2 + usize::from(entry_count) * 12 + 4;
    let mut cursor = 8usize;
    let mut layout = Vec::new();
    for (width, height, rgb, _) in pages {
        assert_eq!(rgb.len(), (*width as usize) * (*height as usize) * 3);
        let ifd = cursor;
        let bits = ifd + ifd_len;
        let pixels = bits + 6;
        cursor = pixels + rgb.len();
        layout.push((ifd, bits, pixels));
    }
    let mut out = vec![0u8; cursor];
    out[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
    out[4..8].copy_from_slice(&(layout[0].0 as u32).to_le_bytes());
    for (index, (width, height, rgb, orientation)) in pages.iter().enumerate() {
        let (ifd, bits, pixels) = layout[index];
        let next = if index + 1 < layout.len() { layout[index + 1].0 as u32 } else { 0 };
        out[ifd..ifd + 2].copy_from_slice(&entry_count.to_le_bytes());
        let entries = [
            tiff_entry(256, 4, 1, *width),
            tiff_entry(257, 4, 1, *height),
            tiff_entry(258, 3, 3, bits as u32),
            tiff_entry(259, 3, 1, 1),
            tiff_entry(262, 3, 1, 2),
            tiff_entry(273, 4, 1, pixels as u32),
            tiff_entry(274, 3, 1, u32::from(*orientation)),
            tiff_entry(277, 3, 1, 3),
            tiff_entry(278, 4, 1, *height),
            tiff_entry(279, 4, 1, rgb.len() as u32),
        ];
        let mut at = ifd + 2;
        for entry in entries {
            out[at..at + 12].copy_from_slice(&entry);
            at += 12;
        }
        out[at..at + 4].copy_from_slice(&next.to_le_bytes());
        out[bits..bits + 6].copy_from_slice(&[8, 0, 8, 0, 8, 0]);
        out[pixels..pixels + rgb.len()].copy_from_slice(rgb);
    }
    out
}

/// Uncompressed 16-bit RGB. Each 8-bit sample is stored in the high byte.
fn rgb16_tiff(pages: &[(u32, u32, &[u8], u16)]) -> Vec<u8> {
    let widened: Vec<(u32, u32, Vec<u8>, u16)> = pages
        .iter()
        .map(|(width, height, rgb, tag)| {
            assert_eq!(rgb.len(), (*width as usize) * (*height as usize) * 3);
            let mut wide = Vec::with_capacity(rgb.len() * 2);
            for byte in *rgb {
                wide.extend_from_slice(&(u16::from(*byte) << 8).to_le_bytes());
            }
            (*width, *height, wide, *tag)
        })
        .collect();
    let entry_count = 10u16;
    let ifd_len = 2 + usize::from(entry_count) * 12 + 4;
    let mut cursor = 8usize;
    let mut layout = Vec::new();
    for (_, _, wide, _) in &widened {
        let ifd = cursor;
        let bits = ifd + ifd_len;
        let pixels = bits + 6;
        cursor = pixels + wide.len();
        layout.push((ifd, bits, pixels));
    }
    let mut out = vec![0u8; cursor];
    out[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
    out[4..8].copy_from_slice(&(layout[0].0 as u32).to_le_bytes());
    for (index, (width, height, wide, orientation)) in widened.iter().enumerate() {
        let (ifd, bits, pixels) = layout[index];
        let next = if index + 1 < layout.len() { layout[index + 1].0 as u32 } else { 0 };
        out[ifd..ifd + 2].copy_from_slice(&entry_count.to_le_bytes());
        let entries = [
            tiff_entry(256, 4, 1, *width),
            tiff_entry(257, 4, 1, *height),
            tiff_entry(258, 3, 3, bits as u32),
            tiff_entry(259, 3, 1, 1),
            tiff_entry(262, 3, 1, 2),
            tiff_entry(273, 4, 1, pixels as u32),
            tiff_entry(274, 3, 1, u32::from(*orientation)),
            tiff_entry(277, 3, 1, 3),
            tiff_entry(278, 4, 1, *height),
            tiff_entry(279, 4, 1, wide.len() as u32),
        ];
        let mut at = ifd + 2;
        for entry in entries {
            out[at..at + 12].copy_from_slice(&entry);
            at += 12;
        }
        out[at..at + 4].copy_from_slice(&next.to_le_bytes());
        out[bits..bits + 6].copy_from_slice(&[16, 0, 16, 0, 16, 0]);
        out[pixels..pixels + wide.len()].copy_from_slice(wide);
    }
    out
}

#[test]
fn a_tiff_with_camera_orientation_is_scanned_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    let stored = image::imageops::rotate270(&scene.image);
    let side = dir.path().join("side.tif");
    fs::write(&side, rgb_tiff(&[(stored.width(), stored.height(), stored.as_raw(), 6)])).unwrap();
    let raw = dir.path().join("raw.tif");
    fs::write(&raw, rgb_tiff(&[(stored.width(), stored.height(), stored.as_raw(), 1)])).unwrap();
    let blank = RgbImage::from_raw(8, 8, vec![0; 8 * 8 * 3]).unwrap();
    let pages = dir.path().join("pages.tif");
    fs::write(
        &pages,
        rgb_tiff(&[
            (blank.width(), blank.height(), blank.as_raw(), 1),
            (scene.image.width(), scene.image.height(), scene.image.as_raw(), 1),
        ]),
    )
    .unwrap();
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf, coverage: Coverage| {
        let out_dir = dir.path().join(format!(
            "out-{}-{}",
            input.file_name().unwrap().to_string_lossy(),
            match coverage {
                Coverage::Complete => "complete",
                Coverage::Measured => "measured",
            }
        ));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let probed = openworld_core::decode::probe(&side).unwrap();
    assert_eq!((probed.width, probed.height), scene.image.dimensions());
    assert!(!probed.video);
    let turned = scan(side, Coverage::Complete);
    assert_eq!(turned.status, "complete", "{}", turned.message);
    assert_eq!(turned.summary, POSSIBLE_CANDIDATE);
    assert!(!turned.candidates.is_empty());
    let sideways = scan(raw, Coverage::Complete);
    assert_eq!(sideways.status, "complete", "{}", sideways.message);
    assert_eq!(sideways.summary, NO_CLEARANCE);
    assert!(sideways.candidates.is_empty());
    let sequence = openworld_core::decode::probe(&pages).unwrap();
    assert!(sequence.video);
    assert_eq!(sequence.frames, 2);
    assert_eq!(sequence.fps, 1.0);
    let later = scan(pages.clone(), Coverage::Complete);
    assert_eq!(later.status, "complete", "{}", later.message);
    assert_eq!(later.summary, POSSIBLE_CANDIDATE);
    assert_eq!(later.frames_decoded, 2);
    assert!(later.candidates.iter().any(|item| item.frame_index == 1));
    assert!(later.candidates.iter().all(|item| item.frame_index == 1));
    let measured = scan(pages, Coverage::Measured);
    assert_eq!(measured.status, "complete", "{}", measured.message);
    assert_eq!(measured.summary, POSSIBLE_CANDIDATE);
    assert_eq!(measured.frames_decoded, 2);
    assert!(measured.candidates.iter().all(|item| item.frame_index == 1));
}

/// Uncompressed CMYK. Ink is zero for none. The black plate stays empty.
fn cmyk_tiff(pages: &[(u32, u32, &[u8], u16)]) -> Vec<u8> {
    let plates: Vec<(u32, u32, Vec<u8>, u16)> = pages
        .iter()
        .map(|(width, height, rgb, tag)| {
            assert_eq!(rgb.len(), (*width as usize) * (*height as usize) * 3);
            let mut ink = Vec::with_capacity(rgb.len() / 3 * 4);
            for pixel in rgb.chunks_exact(3) {
                ink.extend_from_slice(&[255 - pixel[0], 255 - pixel[1], 255 - pixel[2], 0]);
            }
            (*width, *height, ink, *tag)
        })
        .collect();
    let entry_count = 10u16;
    let ifd_len = 2 + usize::from(entry_count) * 12 + 4;
    let mut cursor = 8usize;
    let mut layout = Vec::new();
    for (_, _, ink, _) in &plates {
        let ifd = cursor;
        let bits = ifd + ifd_len;
        let pixels = bits + 8;
        cursor = pixels + ink.len();
        layout.push((ifd, bits, pixels));
    }
    let mut out = vec![0u8; cursor];
    out[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
    out[4..8].copy_from_slice(&(layout[0].0 as u32).to_le_bytes());
    for (index, (width, height, ink, orientation)) in plates.iter().enumerate() {
        let (ifd, bits, pixels) = layout[index];
        let next = if index + 1 < layout.len() { layout[index + 1].0 as u32 } else { 0 };
        out[ifd..ifd + 2].copy_from_slice(&entry_count.to_le_bytes());
        let entries = [
            tiff_entry(256, 4, 1, *width),
            tiff_entry(257, 4, 1, *height),
            tiff_entry(258, 3, 4, bits as u32),
            tiff_entry(259, 3, 1, 1),
            tiff_entry(262, 3, 1, 5),
            tiff_entry(273, 4, 1, pixels as u32),
            tiff_entry(274, 3, 1, u32::from(*orientation)),
            tiff_entry(277, 3, 1, 4),
            tiff_entry(278, 4, 1, *height),
            tiff_entry(279, 4, 1, ink.len() as u32),
        ];
        let mut at = ifd + 2;
        for entry in entries {
            out[at..at + 12].copy_from_slice(&entry);
            at += 12;
        }
        out[at..at + 4].copy_from_slice(&next.to_le_bytes());
        out[bits..bits + 8].copy_from_slice(&[8, 0, 8, 0, 8, 0, 8, 0]);
        out[pixels..pixels + ink.len()].copy_from_slice(ink);
    }
    out
}

#[test]
fn a_cmyk_tiff_is_scanned_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    let stored = image::imageops::rotate270(&scene.image);
    let side = dir.path().join("cmyk-side.tif");
    fs::write(&side, cmyk_tiff(&[(stored.width(), stored.height(), stored.as_raw(), 6)])).unwrap();
    let raw = dir.path().join("cmyk-raw.tif");
    fs::write(&raw, cmyk_tiff(&[(stored.width(), stored.height(), stored.as_raw(), 1)])).unwrap();
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf| {
        let out_dir = dir.path().join(format!("out-{}", input.file_name().unwrap().to_string_lossy()));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Complete,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let turned = scan(side);
    assert_eq!(turned.status, "complete", "{}", turned.message);
    assert_eq!(turned.summary, POSSIBLE_CANDIDATE);
    assert!(!turned.candidates.is_empty());
    let sideways = scan(raw);
    assert_eq!(sideways.status, "complete", "{}", sideways.message);
    assert_eq!(sideways.summary, NO_CLEARANCE);
    assert!(sideways.candidates.is_empty());
}

#[test]
fn a_sixteen_bit_tiff_is_scanned_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    let stored = image::imageops::rotate270(&scene.image);
    let side = dir.path().join("side16.tif");
    fs::write(&side, rgb16_tiff(&[(stored.width(), stored.height(), stored.as_raw(), 6)])).unwrap();
    let raw = dir.path().join("raw16.tif");
    fs::write(&raw, rgb16_tiff(&[(stored.width(), stored.height(), stored.as_raw(), 1)])).unwrap();
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf| {
        let out_dir = dir.path().join(format!("out-{}", input.file_name().unwrap().to_string_lossy()));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Complete,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let turned = scan(side);
    assert_eq!(turned.status, "complete", "{}", turned.message);
    assert_eq!(turned.summary, POSSIBLE_CANDIDATE);
    assert!(!turned.candidates.is_empty());
    let sideways = scan(raw);
    assert_eq!(sideways.status, "complete", "{}", sideways.message);
    assert_eq!(sideways.summary, NO_CLEARANCE);
    assert!(sideways.candidates.is_empty());
}

#[test]
fn a_jpeg_with_camera_orientation_is_scanned_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    let upright = dir.path().join("upright.jpg");
    let mut upright_bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut upright_bytes, 100)
        .write_image(
            scene.image.as_raw(),
            scene.image.width(),
            scene.image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    fs::write(&upright, &upright_bytes).unwrap();
    let stored = image::imageops::rotate270(&scene.image);
    let side = dir.path().join("side.jpg");
    fs::write(&side, jpeg_with_orientation(&stored, 6)).unwrap();
    let raw = dir.path().join("raw.jpg");
    let mut raw_bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut raw_bytes, 100)
        .write_image(stored.as_raw(), stored.width(), stored.height(), image::ExtendedColorType::Rgb8)
        .unwrap();
    fs::write(&raw, &raw_bytes).unwrap();
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf| {
        let out_dir = dir.path().join(format!("out-{}", input.file_name().unwrap().to_string_lossy()));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Complete,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let probed = openworld_core::decode::probe(&side).unwrap();
    assert_eq!((probed.width, probed.height), scene.image.dimensions());
    let turned = scan(side);
    assert_eq!(turned.status, "complete", "{}", turned.message);
    assert_eq!(turned.summary, POSSIBLE_CANDIDATE);
    assert!(!turned.candidates.is_empty());
    let plain = scan(upright);
    assert_eq!(plain.status, "complete", "{}", plain.message);
    assert_eq!(plain.summary, POSSIBLE_CANDIDATE);
    let sideways = scan(raw);
    assert_eq!(sideways.status, "complete", "{}", sideways.message);
    assert_eq!(sideways.summary, NO_CLEARANCE);
    assert!(sideways.candidates.is_empty());
}

#[test]
fn a_webp_with_camera_orientation_is_scanned_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    let stored = image::imageops::rotate270(&scene.image);
    let side = dir.path().join("side.webp");
    fs::write(&side, webp_with_orientation(&stored, 6)).unwrap();
    let raw = dir.path().join("raw.webp");
    let mut raw_bytes = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut raw_bytes)
        .write_image(stored.as_raw(), stored.width(), stored.height(), image::ExtendedColorType::Rgb8)
        .unwrap();
    fs::write(&raw, raw_bytes).unwrap();
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf| {
        let out_dir = dir.path().join(format!("out-{}", input.file_name().unwrap().to_string_lossy()));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Complete,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let probed = openworld_core::decode::probe(&side).unwrap();
    assert_eq!((probed.width, probed.height), scene.image.dimensions());
    assert!(!probed.video);
    let turned = scan(side);
    assert_eq!(turned.status, "complete", "{}", turned.message);
    assert_eq!(turned.summary, POSSIBLE_CANDIDATE);
    assert!(!turned.candidates.is_empty());
    let sideways = scan(raw);
    assert_eq!(sideways.status, "complete", "{}", sideways.message);
    assert_eq!(sideways.summary, NO_CLEARANCE);
    assert!(sideways.candidates.is_empty());
}

#[test]
fn a_video_with_display_rotation_is_scanned_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    scene.image.save(dir.path().join("scene.png")).unwrap();
    let stored = dir.path().join("stored.mp4");
    let shown = dir.path().join("shown.mp4");
    let encode = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-loop", "1", "-i"])
        .arg(dir.path().join("scene.png"))
        .args(["-vf", "transpose=2", "-frames:v", "8", "-r", "10", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&stored)
        .status()
        .expect("ffmpeg");
    assert!(encode.success());
    let tag = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(&stored)
        .args(["-an", "-c:v", "copy", "-bsf:v", "h264_metadata=display_orientation=insert:rotate=-90"])
        .arg(&shown)
        .status()
        .expect("ffmpeg");
    assert!(tag.success());
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf| {
        let out_dir = dir.path().join(format!("out-{}", input.file_name().unwrap().to_string_lossy()));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Complete,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let probed = openworld_core::decode::probe(&shown).unwrap();
    assert_eq!((probed.width, probed.height), scene.image.dimensions());
    let turned = scan(shown);
    assert_eq!(turned.status, "complete", "{}", turned.message);
    assert_eq!(turned.summary, POSSIBLE_CANDIDATE);
    assert!(!turned.candidates.is_empty());
    assert!(turned.frames_decoded >= 1);
    let sideways = scan(stored);
    assert_eq!(sideways.status, "complete", "{}", sideways.message);
    assert_eq!(sideways.summary, NO_CLEARANCE);
    assert!(sideways.candidates.is_empty());
}

#[test]
fn a_video_with_non_square_pixels_is_scanned_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    scene.image.save(dir.path().join("scene.png")).unwrap();
    let wide = dir.path().join("wide.mp4");
    let squashed = dir.path().join("squashed.mp4");
    let encode = |output: &std::path::Path, filter: &str| {
        let status = Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-loop", "1", "-i"])
            .arg(dir.path().join("scene.png"))
            .args(["-vf", filter, "-frames:v", "4", "-r", "10", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(output)
            .status()
            .expect("ffmpeg");
        assert!(status.success());
    };
    encode(&wide, "scale=320:480,setsar=2/1");
    encode(&squashed, "scale=320:480,setsar=1");
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf| {
        let out_dir = dir.path().join(format!("out-{}", input.file_name().unwrap().to_string_lossy()));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Complete,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let probed = openworld_core::decode::probe(&wide).unwrap();
    assert_eq!((probed.width, probed.height), scene.image.dimensions());
    assert!(probed.square_pixels);
    let shown = scan(wide);
    assert_eq!(shown.status, "complete", "{}", shown.message);
    assert_eq!(shown.summary, POSSIBLE_CANDIDATE);
    assert!(!shown.candidates.is_empty());
    let flat = scan(squashed);
    assert_eq!(flat.status, "complete", "{}", flat.message);
    assert_eq!(flat.summary, NO_CLEARANCE);
    assert!(flat.candidates.is_empty());
}

#[test]
fn a_turned_video_with_non_square_pixels_is_scanned_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    scene.image.save(dir.path().join("scene.png")).unwrap();
    let stored = dir.path().join("stored.mp4");
    let shown = dir.path().join("shown.mp4");
    let squeezed = dir.path().join("squeezed.mp4");
    let sideways = dir.path().join("sideways.mp4");
    let encode = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-loop", "1", "-i"])
        .arg(dir.path().join("scene.png"))
        .args(["-vf", "scale=320:480,setsar=2/1,transpose=2", "-frames:v", "4", "-r", "10", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&stored)
        .status()
        .expect("ffmpeg");
    assert!(encode.success());
    let tag = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(&stored)
        .args(["-an", "-c:v", "copy", "-bsf:v", "h264_metadata=display_orientation=insert:rotate=-90"])
        .arg(&shown)
        .status()
        .expect("ffmpeg");
    assert!(tag.success());
    let squeeze = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-loop", "1", "-i"])
        .arg(dir.path().join("scene.png"))
        .args(["-vf", "scale=320:480,setsar=2/1", "-frames:v", "4", "-r", "10", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&squeezed)
        .status()
        .expect("ffmpeg");
    assert!(squeeze.success());
    let extra = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(&squeezed)
        .args(["-an", "-c:v", "copy", "-bsf:v", "h264_metadata=display_orientation=insert:rotate=-90"])
        .arg(&sideways)
        .status()
        .expect("ffmpeg");
    assert!(extra.success());
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf| {
        let out_dir = dir.path().join(format!("out-{}", input.file_name().unwrap().to_string_lossy()));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Complete,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let probed = openworld_core::decode::probe(&shown).unwrap();
    assert_eq!((probed.width, probed.height), scene.image.dimensions());
    assert!(probed.square_pixels);
    let turned = scan(shown);
    assert_eq!(turned.status, "complete", "{}", turned.message);
    assert_eq!(turned.summary, POSSIBLE_CANDIDATE);
    assert!(!turned.candidates.is_empty());
    let raw = scan(stored);
    assert_eq!(raw.status, "complete", "{}", raw.message);
    assert_eq!(raw.summary, NO_CLEARANCE);
    let flagged = scan(sideways);
    assert_eq!(flagged.status, "complete", "{}", flagged.message);
    assert_eq!(flagged.summary, NO_CLEARANCE);
}

#[test]
fn a_video_with_audio_past_the_pictures_stays_complete() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    scene.image.save(dir.path().join("scene.png")).unwrap();
    let pictures = dir.path().join("pictures.mkv");
    let with_audio = dir.path().join("audio-tail.mkv");
    let encode = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-loop", "1", "-i"])
        .arg(dir.path().join("scene.png"))
        .args(["-frames:v", "8", "-r", "10", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&pictures)
        .status()
        .expect("ffmpeg");
    assert!(encode.success());
    let mux = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(&pictures)
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=30",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-map",
            "0:v",
            "-map",
            "1:a",
        ])
        .arg(&with_audio)
        .status()
        .expect("ffmpeg");
    assert!(mux.success());
    let probed = openworld_core::decode::probe(&with_audio).unwrap();
    assert!(!probed.frames_exact, "the container omitted an exact frame count");
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let out_dir = dir.path().join("out");
    let report = scan_path(
        &ScanRequest {
            input: with_audio,
            bundles_dir: repo().join("bundles"),
            bundle_id: "fast".into(),
            posters_dir: pack,
            out_dir,
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            form_factor: FormFactor::Computer,
            execution: Execution::Cpu,
            missing: true,
            wanted: true,
            abort_after_frames: None,
            frames_dir: None,
            media: None,
            now: now(),
        },
        &mut |_| {},
    );
    assert!(probed.duration_sec < 2.0, "picture duration {}", probed.duration_sec);
    assert_eq!(probed.frames, report.frames_decoded);
    let input = dir.path().join("audio-tail.mkv");
    let estimate = openworld_core::scan::estimate_for(&ScanRequest {
        input,
        bundles_dir: repo().join("bundles"),
        bundle_id: "fast".into(),
        posters_dir: dir.path().join("posters"),
        out_dir: dir.path().join("estimate"),
        detection: DetectionSize::Px(640),
        coverage: Coverage::Complete,
        form_factor: FormFactor::Phone,
        execution: Execution::Cpu,
        missing: true,
        wanted: true,
        abort_after_frames: None,
        frames_dir: None,
        media: None,
        now: now(),
    })
    .expect("estimate");
    assert_eq!(estimate.frames_analyzed, report.frames_decoded);
    assert_eq!(estimate.human, "Less than a second");
    assert!(!estimate.suggest_computer);
    assert_eq!(report.status, "complete", "{}", report.message);
    assert_eq!(report.summary, POSSIBLE_CANDIDATE);
    assert!(!report.candidates.is_empty());
}

#[test]
fn an_edit_list_scans_the_frames_a_player_shows() {
    let dir = tempfile::tempdir().unwrap();
    let scene = demo_scene(fast().threshold);
    scene.image.save(dir.path().join("scene.png")).unwrap();
    let scene_video = dir.path().join("scene.mp4");
    let blank_video = dir.path().join("blank.mp4");
    let show_scene = dir.path().join("show-scene.mp4");
    let show_blank = dir.path().join("show-blank.mp4");
    let encode = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-loop", "1", "-i"])
        .arg(dir.path().join("scene.png"))
        .args(["-frames:v", "8", "-r", "10", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&scene_video)
        .status()
        .expect("ffmpeg");
    assert!(encode.success());
    let blank = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=640x480:r=10:d=0.4",
            "-frames:v",
            "4",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&blank_video)
        .status()
        .expect("ffmpeg");
    assert!(blank.success());
    let joined = |name: &str, first: &std::path::Path, second: &std::path::Path| {
        let out = dir.path().join(name);
        let status = Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-i"])
            .arg(first)
            .args(["-i"])
            .arg(second)
            .args(["-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(&out)
            .status()
            .expect("ffmpeg");
        assert!(status.success());
        out
    };
    let trim = |src: &std::path::Path, start: &str, dest: &std::path::Path| {
        let status = Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-ss", start, "-i"])
            .arg(src)
            .args(["-c", "copy"])
            .arg(dest)
            .status()
            .expect("ffmpeg");
        assert!(status.success());
    };
    let hidden_blank = joined("blank-then-scene.mp4", &blank_video, &scene_video);
    let hidden_scene = joined("scene-then-blank.mp4", &scene_video, &blank_video);
    trim(&hidden_blank, "0.4", &show_scene);
    trim(&hidden_scene, "0.8", &show_blank);
    let pack = dir.path().join("posters");
    posters::write_fixture_pack(&pack, now()).unwrap();
    let scan = |input: std::path::PathBuf| {
        let out_dir = dir.path().join(format!("out-{}", input.file_name().unwrap().to_string_lossy()));
        scan_path(
            &ScanRequest {
                input,
                bundles_dir: repo().join("bundles"),
                bundle_id: "fast".into(),
                posters_dir: pack.clone(),
                out_dir,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Complete,
                form_factor: FormFactor::Computer,
                execution: Execution::Cpu,
                missing: true,
                wanted: true,
                abort_after_frames: None,
                frames_dir: None,
                media: None,
                now: now(),
            },
            &mut |_| {},
        )
    };
    let whole = openworld_core::decode::probe(&scene_video).unwrap();
    assert!(whole.frames_exact);
    assert_eq!(whole.frames, 8);
    let plain = scan(scene_video);
    assert_eq!(plain.status, "complete", "{}", plain.message);
    assert_eq!(plain.frames_decoded, 8);
    assert_eq!(plain.summary, POSSIBLE_CANDIDATE);
    let faces: Vec<_> = plain.candidates.iter().filter(|card| card.kind == "face").collect();
    let plates: Vec<_> = plain.candidates.iter().filter(|card| card.kind == "plate").collect();
    assert_eq!(faces.len(), 1);
    assert_eq!(plates.len(), 1);
    assert_eq!(faces[0].poster_id, "fixture-missing-a");
    assert_eq!(plates[0].poster_id, "fixture-plate-c");
    let best = plain
        .comparisons
        .iter()
        .filter(|row| row.passed && row.poster_id == faces[0].poster_id)
        .map(|row| row.cosine)
        .fold(f32::MIN, f32::max);
    assert_eq!(faces[0].cosine, Some(best));
    assert!(plain.inventory.len() > plain.candidates.len());
    assert!(plain.comparisons.len() > plain.candidates.len());
    assert!(plain.inventory.iter().any(|item| item.label == NOT_COMPARED));
    let shown = openworld_core::decode::probe(&show_scene).unwrap();
    let hidden = openworld_core::decode::probe(&hidden_blank).unwrap();
    assert!(shown.frames_exact);
    assert!(shown.frames < hidden.frames);
    let kept = scan(show_scene);
    assert_eq!(kept.status, "complete", "{}", kept.message);
    assert_eq!(kept.summary, POSSIBLE_CANDIDATE);
    assert_eq!(kept.frames_decoded, shown.frames);
    assert!(!kept.candidates.is_empty());
    let dropped = openworld_core::decode::probe(&show_blank).unwrap();
    let full = openworld_core::decode::probe(&hidden_scene).unwrap();
    assert!(dropped.frames < full.frames);
    let gone = scan(show_blank);
    assert_eq!(gone.status, "complete", "{}", gone.message);
    assert_eq!(gone.summary, NO_CLEARANCE);
    assert!(gone.candidates.is_empty());
    assert_eq!(gone.frames_decoded, dropped.frames);
}
