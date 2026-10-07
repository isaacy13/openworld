// SPDX-License-Identifier: Apache-2.0

use image::RgbImage;
use openworld_core::bundle::load_bundles;
use openworld_core::copy::{
    BRIEF_FACE, INCOMPLETE, NO_CLEARANCE, NOT_COMPARED, POSSIBLE_CANDIDATE, VEHICLE_NOT_PERSON,
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
    assert!(report.perception_note.contains("not a SCRFD"));
    let face = report.candidates.iter().find(|c| c.kind == "face").unwrap();
    assert!(face.uncertainty.contains("Fast"));
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
    let _ = pack;
}

#[test]
fn wanted_class_off_skips_that_class_with_the_same_cutoff() {
    let bundle = fast();
    let dir = tempfile::tempdir().unwrap();
    posters::write_fixture_pack(dir.path(), now()).unwrap();
    let pack = posters::load_pack(dir.path(), now()).unwrap();
    let scene = demo_scene(bundle.threshold);
    let mut opts = opts(DetectionSize::Px(640), Coverage::Complete);
    opts.wanted = false;
    opts.fps = 0.0;
    let report = scan_images(&[scene.image], &bundle, &pack, &opts, &mut |_| {});
    assert!(report.candidates.iter().all(|c| c.poster_class == "missing"));
    assert!(report.candidates.iter().all(|c| c.kind != "plate"));
    assert_eq!(report.plates_ocr_attempted, 0);
    assert_eq!(report.threshold, bundle.threshold);
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
