// SPDX-License-Identifier: Apache-2.0

use super::{landmarks_in_crop, Engine, ScanOpts};
use crate::bundle::Bundle;
use crate::copy::{BELOW_CUTOFF, POSSIBLE_CANDIDATE};
use crate::embed::{fixture_probe, unit_embedding};
use crate::estimate::{Coverage, DetectionSize, FormFactor};
use crate::geom::{FrameMap, Rect};
use crate::hardware::Execution;
use crate::posters::write_fixture_pack;
use image::RgbImage;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn bundle() -> Bundle {
    Bundle {
        id: "fast".into(),
        name: "Fast".into(),
        version: "0".into(),
        official: true,
        best_for: "Phones and long video.".into(),
        threshold: 0.55,
        estimate_factor: 1.0,
        detector: "SCRFD-0.5GF".into(),
        detector_version: "test".into(),
        embedder: "ArcFace-MBF".into(),
        embedder_version: "test".into(),
        plate: "RTMDet-nano".into(),
        plate_version: "test".into(),
        plate_license: "Apache-2.0".into(),
        weights: Vec::new(),
        weights_ready: false,
        curve_exists: true,
        real_posters_allowed: false,
        curve_line: "Fixture curve measured. Real FBI photos stay off.".into(),
        dir: PathBuf::from("."),
    }
}

fn opts(out: Option<PathBuf>) -> ScanOpts {
    ScanOpts {
        detection: DetectionSize::Px(640),
        coverage: Coverage::Complete,
        execution: Execution::Cpu,
        form_factor: FormFactor::Phone,
        missing: true,
        wanted: true,
        abort_after_frames: None,
        fps: 30.0,
        frame_count_hint: Some(1),
        warnings: Vec::new(),
        out_dir: out,
    }
}

#[test]
fn a_probe_that_matches_the_poster_is_a_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let pack = write_fixture_pack(
        dir.path(),
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    )
    .unwrap();
    let bundle = bundle();
    let opts = opts(Some(dir.path().join("out")));
    let mut engine = Engine::new(&bundle, &pack, &opts, None);
    let frame = RgbImage::new(160, 160);
    let rect = Rect {
        x: 8,
        y: 8,
        w: 128,
        h: 128,
    };
    let mut labels = Vec::new();
    engine.score_face(
        0,
        &frame,
        rect,
        rect,
        3,
        &unit_embedding(7),
        7,
        None,
        &mut |event| {
            labels.push(event.label);
        },
    );
    assert_eq!(engine.candidates.len(), 1);
    assert_eq!(engine.candidates[0].wording, POSSIBLE_CANDIDATE);
    assert!(engine.candidates[0].uncertainty.starts_with("Score "));
    assert!(engine.candidates[0].uncertainty.contains("Fast keeps a candidate at 0.55 and above."));
    assert!(engine.candidates[0].uncertainty.ends_with("and above."));
    assert!(!engine.candidates[0].uncertainty.contains("Cosine"));
    assert!(!engine.candidates[0].uncertainty.contains(POSSIBLE_CANDIDATE));
    assert_eq!(engine.candidates[0].leaving, crate::copy::LEAVING);
    assert!(engine.candidates[0]
        .fbi_url
        .starts_with("https://www.fbi.gov"));
    assert_eq!(engine.inventory[0].label, POSSIBLE_CANDIDATE);
    assert_eq!(labels, vec![POSSIBLE_CANDIDATE]);
    let report = engine.finish();
    assert_eq!(report.summary, POSSIBLE_CANDIDATE);
    assert_eq!(report.status, "complete");
}

#[test]
fn repeated_frames_of_one_track_keep_the_strongest_card() {
    let dir = tempfile::tempdir().unwrap();
    let pack = write_fixture_pack(
        dir.path(),
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    )
    .unwrap();
    let bundle = bundle();
    let opts = opts(None);
    let mut engine = Engine::new(&bundle, &pack, &opts, None);
    let frame = RgbImage::new(160, 160);
    let rect = Rect {
        x: 8,
        y: 8,
        w: 128,
        h: 128,
    };
    let mut weaker = None;
    for index in 0..40 {
        let (probe, score) = fixture_probe(7, 16, 16, 128, 128, index);
        if score >= bundle.threshold && score < 0.95 {
            weaker = Some(probe);
            break;
        }
    }
    let weaker = weaker.expect("a passing probe under the clean poster vector");
    let strong = unit_embedding(7);
    let score = |engine: &mut Engine, index: u64, probe: &[f32], id: u16| {
        engine.score_face(index, &frame, rect, rect, 3, probe, id, None, &mut |_| {});
    };
    score(&mut engine, 2, &weaker, 7);
    score(&mut engine, 8, &strong, 7);
    score(&mut engine, 1, &strong, 7);
    score(&mut engine, 6, &unit_embedding(99), 99);
    assert!(engine.candidates.len() > 1);
    assert!(engine.comparisons.len() > engine.candidates.len());
    assert_eq!(engine.inventory.len(), 4);
    let report = engine.finish();
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].frame_index, 1);
    assert_eq!(report.candidates[0].frame_label, "Frame 2.");
    assert_eq!(report.candidates[0].track_id, 3);
    assert_eq!(report.candidates[0].poster_id, "fixture-missing-a");
    assert!(report.candidates[0].cosine.unwrap() > bundle.threshold);
    assert!(report.comparisons.iter().any(|row| row.frame_index == 6 && !row.passed));
    assert!(report
        .comparisons
        .iter()
        .any(|row| row.frame_index == 2 && row.passed));
    assert_eq!(report.inventory.len(), 4);
    assert_eq!(report.summary, POSSIBLE_CANDIDATE);
}

#[test]
fn an_impostor_probe_stays_below_the_cutoff() {
    let dir = tempfile::tempdir().unwrap();
    let pack = write_fixture_pack(
        dir.path(),
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    )
    .unwrap();
    let bundle = bundle();
    let opts = opts(None);
    let mut engine = Engine::new(&bundle, &pack, &opts, None);
    let frame = RgbImage::new(160, 160);
    let rect = Rect {
        x: 8,
        y: 8,
        w: 128,
        h: 128,
    };
    engine.score_face(
        0,
        &frame,
        rect,
        rect,
        4,
        &unit_embedding(99),
        99,
        None,
        &mut |_| {},
    );
    assert!(engine.candidates.is_empty());
    assert!(engine.comparisons.iter().all(|row| !row.passed));
    assert!(engine
        .comparisons
        .iter()
        .all(|row| row.poster_fiducial_id != Some(row.fiducial_id)));
    assert_eq!(engine.inventory[0].label, BELOW_CUTOFF);
    let report = engine.finish();
    assert_eq!(report.summary, crate::copy::NO_CLEARANCE);
}

#[test]
fn a_turned_off_class_is_not_scored() {
    let dir = tempfile::tempdir().unwrap();
    let pack = write_fixture_pack(
        dir.path(),
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    )
    .unwrap();
    let bundle = bundle();
    let mut opts = opts(None);
    opts.wanted = false;
    let mut engine = Engine::new(&bundle, &pack, &opts, None);
    let frame = RgbImage::new(160, 160);
    let rect = Rect {
        x: 8,
        y: 8,
        w: 128,
        h: 128,
    };
    engine.score_face(
        0,
        &frame,
        rect,
        rect,
        5,
        &unit_embedding(11),
        11,
        None,
        &mut |_| {},
    );
    assert!(engine.candidates.is_empty());
    assert!(engine
        .comparisons
        .iter()
        .all(|row| row.poster_fiducial_id != Some(11)));
}

#[test]
fn a_detector_failure_is_a_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let pack = write_fixture_pack(
        dir.path(),
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    )
    .unwrap();
    let bundle = bundle();
    let opts = opts(None);
    let mut engine = Engine::new(&bundle, &pack, &opts, None);
    engine.runtime_failure = Some("The detector did not load. Refusing.".into());
    let report = engine.finish();
    assert_eq!(report.refusal.as_deref(), Some("missing_weights"));
    assert_ne!(report.summary, crate::copy::NO_CLEARANCE);
}

#[test]
fn a_stopped_scan_without_a_reason_is_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let pack = write_fixture_pack(
        dir.path(),
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    )
    .unwrap();
    let bundle = bundle();
    let opts = opts(None);
    let mut engine = Engine::new(&bundle, &pack, &opts, None);
    engine.stopped = true;
    let report = engine.finish();
    assert_eq!(report.status, "incomplete");
    assert_eq!(report.summary, crate::copy::INCOMPLETE);
}

#[test]
fn landmarks_move_into_the_original_crop() {
    let map = FrameMap { det_to_orig: 2.0 };
    let points = [(10.0, 20.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)];
    let orig = Rect {
        x: 4,
        y: 6,
        w: 40,
        h: 40,
    };
    let local = landmarks_in_crop(map, points, orig);
    assert_eq!(local[0], (16.0, 34.0));
}
