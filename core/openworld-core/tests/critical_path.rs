// SPDX-License-Identifier: Apache-2.0

//! Critical-path decisions: refuse, incomplete, candidate, clearance, and the locked cutoff.
//! Snapshot file: tests/snapshots/decisions.json

use openworld_core::bundle::{load_bundle, load_bundles};
use openworld_core::copy::{
    product_copy, BRIEF_FACE, INCOMPLETE, NOT_COMPARED, NO_CLEARANCE, TIMESTAMP_DISAGREE,
};
use openworld_core::decode::load_frame_dir;
use openworld_core::embed::fixture_probe;
use openworld_core::estimate::{Coverage, DetectionSize, FormFactor};
use openworld_core::fiducial::{self, render_face_module, render_plate};
use openworld_core::geom::{blank, resize_long_side};
use openworld_core::hardware::Execution;
use openworld_core::invoke::invoke_json;
use openworld_core::measure_fast;
use openworld_core::posters::{self, sha256_hex, PosterClass};
use openworld_core::scan::{
    delete_output, estimate_for, media_warnings, scan_images, scan_path, MediaFacts, ScanOpts,
    ScanRequest,
};
use openworld_core::scene::demo_scene;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fast() -> openworld_core::Bundle {
    load_bundles(&repo().join("bundles"))
        .unwrap()
        .into_iter()
        .find(|b| b.id == "fast")
        .unwrap()
}

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_760_000_000)
}

fn request(
    dir: &Path,
    input: PathBuf,
    bundle: &str,
    posters: PathBuf,
    extra: RequestExtra,
) -> ScanRequest {
    ScanRequest {
        input,
        bundles_dir: repo().join("bundles"),
        bundle_id: bundle.into(),
        posters_dir: posters,
        out_dir: dir.join("out"),
        detection: DetectionSize::Px(640),
        coverage: extra.coverage,
        form_factor: FormFactor::Phone,
        execution: Execution::Cpu,
        missing: extra.missing,
        wanted: extra.wanted,
        abort_after_frames: extra.abort,
        frames_dir: extra.frames,
        media: extra.media,
        now: now(),
    }
}

struct RequestExtra {
    coverage: Coverage,
    missing: bool,
    wanted: bool,
    abort: Option<u64>,
    frames: Option<PathBuf>,
    media: Option<MediaFacts>,
}

impl Default for RequestExtra {
    fn default() -> Self {
        Self {
            coverage: Coverage::Complete,
            missing: true,
            wanted: true,
            abort: None,
            frames: None,
            media: None,
        }
    }
}

fn pack(dir: &Path) -> PathBuf {
    let posters = dir.join("posters");
    posters::write_fixture_pack(&posters, now()).unwrap();
    posters
}

fn save_face(dir: &Path, id: u16, module: u32, x: u32, y: u32) -> PathBuf {
    let marker = render_face_module(id, module);
    let (w, h) = marker.dimensions();
    let mut image = blank(400, 320);
    fiducial::place(&mut image, &marker, x, y);
    let path = dir.join(format!("face-{id}-{module}.png"));
    image.save(&path).unwrap();
    let _ = (w, h);
    path
}

fn decisions(report: &openworld_core::ScanReport) -> Value {
    json!({
        "status": report.status,
        "summary": report.summary,
        "inventory": report.inventory.iter().map(|item| format!("{}:{}", item.kind, item.label)).collect::<Vec<_>>(),
        "candidates": report.candidates.iter().map(|item| json!({
            "kind": item.kind,
            "wording": item.wording,
            "leaving": item.leaving,
            "poster_class": item.poster_class,
        })).collect::<Vec<_>>(),
        "faces_seen_not_compared": report.faces_seen_not_compared,
    })
}

#[test]
fn decision_snapshot_matches_the_locked_cutoff() {
    let fast = fast();
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let scene = demo_scene(fast.threshold);
    let scene_path = dir.path().join("scene.png");
    scene.image.save(&scene_path).unwrap();
    let scene_report = scan_path(
        &request(
            dir.path(),
            scene_path,
            "fast",
            posters.clone(),
            RequestExtra::default(),
        ),
        &mut |_| {},
    );

    let (x, y) = below_origin(7, 16, fast.threshold);
    let below_path = save_face(dir.path(), 7, 16, x, y);
    let below = scan_path(
        &request(
            dir.path(),
            below_path,
            "fast",
            posters.clone(),
            RequestExtra::default(),
        ),
        &mut |_| {},
    );

    let impostor_path = save_face(dir.path(), 99, 16, 16, 16);
    let impostor = scan_path(
        &request(
            dir.path(),
            impostor_path,
            "fast",
            posters.clone(),
            RequestExtra::default(),
        ),
        &mut |_| {},
    );

    let blank_path = dir.path().join("blank.png");
    blank(400, 320).save(&blank_path).unwrap();
    let blank_report = scan_path(
        &request(
            dir.path(),
            blank_path,
            "fast",
            posters.clone(),
            RequestExtra::default(),
        ),
        &mut |_| {},
    );

    let tiny_path = save_face(dir.path(), 7, 4, 16, 16);
    let tiny = scan_path(
        &request(
            dir.path(),
            tiny_path,
            "fast",
            posters,
            RequestExtra::default(),
        ),
        &mut |_| {},
    );

    let got = json!({
        "scene": decisions(&scene_report),
        "below_cutoff": decisions(&below),
        "impostor": decisions(&impostor),
        "blank": decisions(&blank_report),
        "below_64": decisions(&tiny),
    });
    let text = serde_json::to_string_pretty(&got).unwrap() + "\n";
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/decisions.json");
    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &text).unwrap();
    }
    let expected = fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        text,
        expected,
        "decision snapshot drifted; set UPDATE_SNAPSHOTS=1 to rewrite {}",
        path.display()
    );
}

fn below_origin(id: u16, module: u32, threshold: f32) -> (u32, u32) {
    let marker = render_face_module(id, module);
    let (w, h) = marker.dimensions();
    for y in (8..80).step_by(2) {
        for x in (8..80).step_by(2) {
            let (_probe, score) = fixture_probe(id, x, y, w, h, 0);
            if score < threshold {
                return (x, y);
            }
        }
    }
    panic!("no below-cutoff placement");
}

#[test]
fn missing_bundle_missing_pack_and_bad_perception_refuse() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    blank(32, 32).save(&input).unwrap();
    let missing_bundle = scan_path(
        &request(
            dir.path(),
            input.clone(),
            "no-such",
            dir.path().join("none"),
            RequestExtra::default(),
        ),
        &mut |_| {},
    );
    assert_eq!(missing_bundle.refusal.as_deref(), Some("bundle_not_found"));
    assert_eq!(
        missing_bundle.summary,
        "That bundle is not in the catalog. Refusing."
    );
    assert_ne!(missing_bundle.summary, NO_CLEARANCE);

    let missing_pack = scan_path(
        &request(
            dir.path(),
            input.clone(),
            "fast",
            dir.path().join("none"),
            RequestExtra::default(),
        ),
        &mut |_| {},
    );
    assert_eq!(missing_pack.refusal.as_deref(), Some("missing_pack"));

    let posters = pack(dir.path());
    let mut value: Value =
        serde_json::from_slice(&fs::read(posters.join("snapshot.json")).unwrap()).unwrap();
    value["perception"] = json!("not-a-perception");
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(posters.join("snapshot.json"), &bytes).unwrap();
    fs::write(
        posters.join("snapshot.sha256"),
        format!("{}\n", sha256_hex(&bytes)),
    )
    .unwrap();
    let bad = scan_path(
        &request(dir.path(), input, "fast", posters, RequestExtra::default()),
        &mut |_| {},
    );
    assert_eq!(bad.refusal.as_deref(), Some("bad_hash"));
    assert!(bad.message.contains("perception"));
}

#[test]
fn a_text_file_refuses_and_a_stopped_scan_is_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let junk = dir.path().join("notes.txt");
    fs::write(&junk, b"not a photo").unwrap();
    let refused = scan_path(
        &request(
            dir.path(),
            junk,
            "fast",
            posters.clone(),
            RequestExtra::default(),
        ),
        &mut |_| {},
    );
    assert_eq!(refused.refusal.as_deref(), Some("bad_codec"));
    assert_ne!(refused.summary, NO_CLEARANCE);
    assert!(refused.disclosure.iter().all(|line| line != NO_CLEARANCE));
    assert!(refused
        .disclosure
        .iter()
        .any(|line| line.contains("Nothing is uploaded.")));

    let still = save_face(dir.path(), 7, 16, 16, 16);
    let mut extra = RequestExtra::default();
    extra.abort = Some(0);
    let stopped = scan_path(
        &request(dir.path(), still, "fast", posters, extra),
        &mut |_| {},
    );
    assert_eq!(stopped.status, "incomplete");
    assert_eq!(stopped.summary, INCOMPLETE);
    assert!(stopped.disclosure.iter().all(|line| line != NO_CLEARANCE));
}

#[test]
fn several_frames_without_a_frame_rate_refuse() {
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let frames = dir.path().join("frames");
    fs::create_dir_all(&frames).unwrap();
    blank(64, 64).save(frames.join("frame_000000.png")).unwrap();
    blank(64, 64).save(frames.join("frame_000001.png")).unwrap();
    let input = dir.path().join("clip.png");
    fs::write(&input, b"x").unwrap();
    let mut extra = RequestExtra::default();
    extra.frames = Some(frames);
    extra.media = Some(MediaFacts {
        width: 64,
        height: 64,
        fps: 0.0,
        frames: 2,
        duration_sec: 0.0,
        video: true,
        container_unix: None,
    });
    let report = scan_path(
        &request(dir.path(), input, "fast", posters, extra),
        &mut |_| {},
    );
    assert_eq!(report.refusal.as_deref(), Some("bad_codec"));
    assert!(report.message.contains("frame rate"));
}

#[test]
fn measured_coverage_names_the_brief_face_banner() {
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let input = save_face(dir.path(), 7, 4, 16, 16);
    let mut extra = RequestExtra::default();
    extra.coverage = Coverage::Measured;
    let report = scan_path(
        &request(dir.path(), input, "fast", posters, extra),
        &mut |_| {},
    );
    assert_eq!(report.coverage_banner.as_deref(), Some(BRIEF_FACE));
}

#[test]
fn disagreeing_timestamps_warn_and_do_not_refuse() {
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let input = dir.path().join("old.png");
    blank(80, 80).save(&input).unwrap();
    let mtime = now() - Duration::from_secs(40 * 24 * 3600);
    fs::File::options()
        .write(true)
        .open(&input)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    let warnings = media_warnings(
        Some(mtime),
        Some(mtime - Duration::from_secs(3 * 24 * 3600)),
        now(),
    );
    assert!(warnings
        .iter()
        .any(|line| line.contains("older than about 30 days")));
    assert!(warnings.iter().any(|line| line == TIMESTAMP_DISAGREE));
    let mut extra = RequestExtra::default();
    extra.media = Some(MediaFacts {
        width: 80,
        height: 80,
        fps: 0.0,
        frames: 1,
        duration_sec: 0.0,
        video: false,
        container_unix: Some(1_000_000_000),
    });
    let frames = dir.path().join("one");
    fs::create_dir_all(&frames).unwrap();
    fs::copy(&input, frames.join("frame_000000.png")).unwrap();
    extra.frames = Some(frames);
    let report = scan_path(
        &request(dir.path(), input, "fast", posters, extra),
        &mut |_| {},
    );
    assert_eq!(report.status, "complete");
    assert!(report
        .warnings
        .iter()
        .any(|line| line == TIMESTAMP_DISAGREE));
}

#[test]
fn a_small_plate_is_not_compared_and_missing_class_off_skips_that_face() {
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let mut image = blank(400, 200);
    let plate = render_plate("FIX123", 2).unwrap();
    fiducial::place(&mut image, &plate, 8, 8);
    let path = dir.path().join("plate.png");
    image.save(&path).unwrap();
    let report = scan_path(
        &request(
            dir.path(),
            path,
            "fast",
            posters.clone(),
            RequestExtra::default(),
        ),
        &mut |_| {},
    );
    assert!(report
        .inventory
        .iter()
        .any(|item| item.kind == "plate" && item.label == NOT_COMPARED));
    assert!(report.candidates.is_empty());

    let fast = fast();
    let scene_path = dir.path().join("scene.png");
    demo_scene(fast.threshold).image.save(&scene_path).unwrap();
    let mut extra = RequestExtra::default();
    extra.missing = false;
    let skipped = scan_path(
        &request(dir.path(), scene_path, "fast", posters, extra),
        &mut |_| {},
    );
    assert!(skipped
        .candidates
        .iter()
        .all(|item| item.poster_class != PosterClass::Missing.as_str()));
    assert!(skipped.candidates.iter().any(|item| item.kind == "plate"));
}

#[test]
fn estimate_refuses_a_zero_size_and_an_unknown_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    blank(20, 20).save(&input).unwrap();
    let mut extra = RequestExtra::default();
    extra.media = Some(MediaFacts {
        width: 0,
        height: 10,
        fps: 0.0,
        frames: 1,
        duration_sec: 0.0,
        video: false,
        container_unix: None,
    });
    let bad = estimate_for(&request(
        dir.path(),
        input.clone(),
        "fast",
        dir.path().join("p"),
        extra,
    ))
    .unwrap_err();
    assert_eq!(bad.refusal.as_deref(), Some("bad_codec"));
    let unknown = estimate_for(&request(
        dir.path(),
        input,
        "nope",
        dir.path().join("p"),
        RequestExtra::default(),
    ))
    .unwrap_err();
    assert_eq!(unknown.refusal.as_deref(), Some("bundle_not_found"));
    assert_eq!(
        unknown.summary,
        "That bundle is not in the catalog. Refusing."
    );
}

#[test]
fn deleting_a_result_requires_the_report_file() {
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let input = dir.path().join("blank.png");
    blank(40, 40).save(&input).unwrap();
    let report = scan_path(
        &request(dir.path(), input, "fast", posters, RequestExtra::default()),
        &mut |_| {},
    );
    assert_eq!(report.status, "complete");
    assert!(dir.path().join("out/result.json").is_file());
    delete_output(&dir.path().join("out")).unwrap();
    assert!(!dir.path().join("out").exists());
    let err = delete_output(dir.path()).unwrap_err();
    assert!(err.contains("Refusing"));
}

#[test]
fn the_output_directory_cannot_be_created_under_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let input = dir.path().join("blank.png");
    blank(40, 40).save(&input).unwrap();
    let blocker = dir.path().join("blocked");
    fs::write(&blocker, b"not a directory").unwrap();
    let mut req = request(dir.path(), input, "fast", posters, RequestExtra::default());
    req.out_dir = blocker.join("out");
    let report = scan_path(&req, &mut |_| {});
    assert_eq!(report.status, "refused");
    assert_eq!(
        report.summary,
        "The output directory could not be created. Refusing."
    );
    assert_eq!(report.message, report.summary);
}

#[test]
fn the_output_directory_cannot_be_replaced_when_it_cannot_be_removed() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let input = dir.path().join("blank.png");
    blank(40, 40).save(&input).unwrap();
    let out = dir.path().join("stuck");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("keep"), b"x").unwrap();
    let mut perms = fs::metadata(&out).unwrap().permissions();
    perms.set_mode(0o555);
    fs::set_permissions(&out, perms).unwrap();
    assert!(
        fs::remove_file(out.join("keep")).is_err(),
        "this account can still remove a file from a mode-555 directory"
    );
    let mut req = request(dir.path(), input, "fast", posters, RequestExtra::default());
    req.out_dir = out.clone();
    let report = scan_path(&req, &mut |_| {});
    let mut perms = fs::metadata(&out).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&out, perms).unwrap();
    assert_eq!(report.status, "refused");
    assert_eq!(
        report.summary,
        "The output directory could not be replaced. Refusing."
    );
    assert_eq!(report.message, report.summary);
}

#[test]
fn an_onnx_pack_without_weights_refuses_before_a_clearance() {
    let fast = fast();
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let mut value: Value =
        serde_json::from_slice(&fs::read(posters.join("snapshot.json")).unwrap()).unwrap();
    value["perception"] = json!("onnx");
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(posters.join("snapshot.json"), &bytes).unwrap();
    fs::write(
        posters.join("snapshot.sha256"),
        format!("{}\n", sha256_hex(&bytes)),
    )
    .unwrap();
    let loaded = posters::load_pack(&posters, now()).unwrap();
    let opts = ScanOpts {
        detection: DetectionSize::Px(640),
        coverage: Coverage::Complete,
        execution: Execution::Cpu,
        form_factor: FormFactor::Computer,
        missing: true,
        wanted: true,
        abort_after_frames: None,
        fps: 0.0,
        frame_count_hint: Some(1),
        warnings: Vec::new(),
        out_dir: None,
    };
    let report = scan_images(&[blank(80, 80)], &fast, &loaded, &opts, &mut |_| {});
    assert_eq!(report.refusal.as_deref(), Some("missing_weights"));
    assert_ne!(report.summary, NO_CLEARANCE);
}

#[test]
fn product_sentences_and_catalog_edges_stay_closed() {
    let copy = product_copy();
    for key in [
        "possible_candidate",
        "not_compared",
        "incomplete",
        "no_clearance",
        "leaving",
    ] {
        assert!(copy[key].as_str().unwrap().len() > 3, "{key}");
    }
    assert_eq!(copy["no_class"].as_str(), Some("No class was on."));
    assert!(copy["size_hint"]
        .as_str()
        .unwrap()
        .contains("A face under 64 px on that image is left out."));
    assert!(!copy["size_hint"].as_str().unwrap().contains("about 112"));
    assert_eq!(
        copy["no_camera"].as_str(),
        Some("Import a file you already have. There is no camera.")
    );
    assert_eq!(openworld_core::copy::class_note(false, true), "Wanted.");
    assert_eq!(
        openworld_core::copy::estimate_class_line(true, true),
        "Missing and wanted."
    );
    assert_eq!(
        openworld_core::copy::estimate_class_line(true, false),
        "Missing."
    );
    assert_eq!(
        openworld_core::copy::estimate_class_line(false, false),
        "Choose missing, wanted, or both."
    );
    assert_eq!(
        openworld_core::posters::PosterClass::Missing.label(),
        "Missing"
    );
    assert_eq!(
        openworld_core::posters::PosterClass::Wanted.label(),
        "Wanted"
    );
    assert!(Coverage::parse("sometimes").is_none());
    assert!(DetectionSize::parse("100").is_none());
    assert_eq!(DetectionSize::Px(100).label(), "Custom");

    let bad = load_frame_dir(Path::new("/tmp/openworld-no-such-frames"));
    assert!(bad.is_err());
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("notes.txt"), b"skip").unwrap();
    assert!(load_frame_dir(dir.path()).is_err());
    blank(16, 16)
        .save(dir.path().join("frame_000000.png"))
        .unwrap();
    assert_eq!(load_frame_dir(dir.path()).unwrap().len(), 1);

    let (image, map) = resize_long_side(&blank(100, 80), 30);
    assert!(image.width() < 100);
    assert!(map.det_to_orig > 1.0);

    let bundle_dir = tempfile::tempdir().unwrap();
    fs::write(
        bundle_dir.path().join("manifest.toml"),
        "schema = \"nope\"\n",
    )
    .unwrap();
    assert!(load_bundle(bundle_dir.path()).is_err());
}

#[test]
fn the_library_entry_refuses_a_broken_request() {
    for text in [
        "{",
        "{\"argv\":[1]}",
        "{\"nope\":true}",
        "{\"argv\":[\"dance\"]}",
    ] {
        let value: Value = serde_json::from_str(&invoke_json(text)).unwrap();
        assert_eq!(value["status"], "refused");
        assert_ne!(value["summary"], NO_CLEARANCE);
    }
}

#[test]
fn the_c_entry_refuses_an_empty_pointer() {
    let ptr = openworld_core::ffi::ow_command(std::ptr::null());
    let text = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    openworld_core::ffi::ow_string_free(ptr);
    openworld_core::ffi::ow_string_free(std::ptr::null_mut());
    let scan = openworld_core::ffi::ow_scan_request(std::ptr::null());
    let scan_text = unsafe { std::ffi::CStr::from_ptr(scan) }
        .to_string_lossy()
        .into_owned();
    openworld_core::ffi::ow_string_free(scan);
    assert!(text.contains("Refusing"));
    assert!(scan_text.contains("Refusing"));
}

#[test]
fn the_fast_curve_counts_add_up_and_real_photos_stay_off() {
    let fast = fast();
    assert!(!fast.real_posters_allowed);
    let measured = measure_fast(&fast);
    assert!(!measured.schema.is_empty());
    assert_eq!(measured.perception, "fiducial-v1");
    assert_eq!(
        measured.true_positive + measured.false_negative,
        measured.genuine_cosines.len() as u32
    );
    assert_eq!(
        measured.false_positive + measured.true_negative,
        measured.impostor_cosines.len() as u32
    );
    assert_eq!(measured.false_positive, 0);
    assert_eq!(measured.below_64_kept, 0);
    assert_eq!(measured.plate_unpublished_candidate, 0);
    assert_eq!(measured.genuine_not_compared, 0);
    assert_eq!(measured.impostor_not_compared, 0);
}

#[test]
fn an_expired_pack_refuses_and_a_newer_container_still_warns() {
    let dir = tempfile::tempdir().unwrap();
    let posters = pack(dir.path());
    let mut value: Value =
        serde_json::from_slice(&fs::read(posters.join("snapshot.json")).unwrap()).unwrap();
    value["expires_at"] = json!("2000-01-01T00:00:00Z");
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(posters.join("snapshot.json"), &bytes).unwrap();
    fs::write(
        posters.join("snapshot.sha256"),
        format!("{}\n", sha256_hex(&bytes)),
    )
    .unwrap();
    let input = dir.path().join("in.png");
    blank(32, 32).save(&input).unwrap();
    let expired = scan_path(
        &request(dir.path(), input, "fast", posters, RequestExtra::default()),
        &mut |_| {},
    );
    assert_eq!(expired.refusal.as_deref(), Some("expired_pack"));
    assert_ne!(expired.summary, NO_CLEARANCE);

    let file_time = now() - Duration::from_secs(10 * 24 * 3600);
    let warnings = media_warnings(
        Some(file_time),
        Some(file_time + Duration::from_secs(3 * 24 * 3600)),
        now(),
    );
    assert!(warnings.iter().any(|line| line == TIMESTAMP_DISAGREE));
}

#[test]
fn an_estimate_uses_the_platform_duration_or_the_frame_rate() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    blank(80, 60).save(&input).unwrap();
    let still = estimate_for(&request(
        dir.path(),
        input.clone(),
        "fast",
        dir.path().join("p"),
        RequestExtra::default(),
    ))
    .unwrap();
    assert!(!still.human.is_empty());
    assert!(still.device_note.unwrap().contains("CPU"));

    let mut video = RequestExtra::default();
    video.media = Some(MediaFacts {
        width: 320,
        height: 180,
        fps: 10.0,
        frames: 50,
        duration_sec: 5.0,
        video: true,
        container_unix: None,
    });
    let timed = estimate_for(&request(
        dir.path(),
        input.clone(),
        "fast",
        dir.path().join("p"),
        video,
    ))
    .unwrap();
    assert!(!timed.human.is_empty());

    let mut rated = RequestExtra::default();
    rated.media = Some(MediaFacts {
        width: 320,
        height: 180,
        fps: 10.0,
        frames: 40,
        duration_sec: 0.0,
        video: true,
        container_unix: None,
    });
    let from_rate = estimate_for(&request(
        dir.path(),
        input,
        "fast",
        dir.path().join("p"),
        rated,
    ))
    .unwrap();
    assert!(!from_rate.human.is_empty());
}
