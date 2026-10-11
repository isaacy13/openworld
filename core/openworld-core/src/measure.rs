// SPDX-License-Identifier: Apache-2.0

//! Runs the Fast bundle's locked cutoff on fixture markers.
//! Python in `eval/` turns these counts into FMR, FNMR, and confidence intervals.
//! This is not an SCRFD measurement and it does not use real faces.

use crate::bundle::Bundle;
use crate::embed::unit_embedding;
use crate::estimate::{Coverage, DetectionSize, FormFactor};
use crate::fiducial::{self, PERCEPTION_FIDUCIAL};
use crate::geom::blank;
use crate::hardware::Execution;
use crate::posters::{Poster, PosterClass, PosterPack, SCHEMA};
use crate::scan::{scan_images, ScanOpts};
use image::RgbImage;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Measurement {
    pub schema: String,
    pub bundle_id: String,
    pub bundle_name: String,
    pub threshold: f32,
    pub hardware: String,
    pub preprocess: String,
    pub perception: String,
    pub detector_declared: String,
    pub embedder_declared: String,
    pub genuine_cosines: Vec<f32>,
    pub impostor_cosines: Vec<f32>,
    /// Compared genuine pairs at or above the locked cutoff.
    pub true_positive: u32,
    /// Compared genuine pairs below the locked cutoff.
    pub false_negative: u32,
    /// Compared impostor pairs at or above the locked cutoff.
    pub false_positive: u32,
    /// Compared impostor pairs below the locked cutoff.
    pub true_negative: u32,
    pub genuine_not_compared: u32,
    pub impostor_not_compared: u32,
    pub genuine_undetected: u32,
    pub impostor_undetected: u32,
    pub genuine_wrong_identity: u32,
    pub impostor_wrong_identity: u32,
    pub plate_match_candidate: u32,
    pub plate_match_trials: u32,
    pub plate_mismatch_candidate: u32,
    pub plate_mismatch_trials: u32,
    pub plate_unpublished_candidate: u32,
    pub plate_unpublished_trials: u32,
    pub detection_hit: u32,
    pub detection_miss: u32,
    pub below_64_kept: u32,
    pub faces_seen_not_compared: u32,
    pub miss_1s_complete_hit: u32,
    pub miss_1s_complete_miss: u32,
    pub miss_1s_measured_hit: u32,
    pub miss_1s_measured_miss: u32,
    pub miss_brief_complete_hit: u32,
    pub miss_brief_complete_miss: u32,
    pub miss_brief_measured_hit: u32,
    pub miss_brief_measured_miss: u32,
}

pub fn measure_fast(bundle: &Bundle) -> Measurement {
    let mut genuine_cosines = Vec::new();
    let mut impostor_cosines = Vec::new();
    let mut detection_hit = 0u32;
    let mut detection_miss = 0u32;
    let mut below_64_kept = 0u32;
    let mut faces_seen_not_compared = 0u32;

    for n in 0..40 {
        let image = face_at(7, 8, 12 + n * 3, 10 + (n % 4) * 2, 240, 180);
        let report = scan_images(&[image], bundle, &pack_with(7), &still_opts(DetectionSize::Px(640)), &mut |_| {});
        if report.inventory.iter().any(|item| item.kind == "face") {
            detection_hit += 1;
        } else {
            detection_miss += 1;
        }
    }
    for n in 0..20 {
        let image = face_at(7, 4, 16 + n * 4, 16, 240, 180);
        let report = scan_images(&[image], bundle, &pack_with(7), &still_opts(DetectionSize::Full), &mut |_| {});
        if report.inventory.iter().any(|item| item.kind == "face") {
            below_64_kept += 1;
        }
    }
    for n in 0..20 {
        let image = face_at(7, 8, 20 + n * 5, 18, 240, 180);
        let report = scan_images(&[image], bundle, &pack_with(7), &still_opts(DetectionSize::Full), &mut |_| {});
        faces_seen_not_compared += report.faces_seen_not_compared as u32;
    }
    let mut true_positive = 0u32;
    let mut false_negative = 0u32;
    let mut false_positive = 0u32;
    let mut true_negative = 0u32;
    let mut genuine_not_compared = 0u32;
    let mut impostor_not_compared = 0u32;
    let mut genuine_undetected = 0u32;
    let mut impostor_undetected = 0u32;
    let mut genuine_wrong_identity = 0u32;
    let mut impostor_wrong_identity = 0u32;
    let places = comparison_places();
    // Each genuine trial is one fixture identity against its own poster, at a new placement.
    for id in 1u16..=16 {
        for (x, y) in places {
            let image = face_at(id, 16, x, y, 400, 320);
            let report = scan_images(&[image], bundle, &pack_with(id), &still_opts(DetectionSize::Px(640)), &mut |_| {});
            record_pair(
                &report,
                true,
                id,
                &mut genuine_cosines,
                &mut true_positive,
                &mut false_negative,
                &mut genuine_not_compared,
                &mut genuine_undetected,
                &mut genuine_wrong_identity,
            );
        }
    }
    // Each impostor trial is a different probe identity against that gallery poster.
    for gallery in 1u16..=16 {
        for k in 0..places.len() {
            let probe = 200 + gallery * 16 + k as u16;
            let (x, y) = places[k];
            let image = face_at(probe, 16, x, y, 400, 320);
            let report = scan_images(&[image], bundle, &pack_with(gallery), &still_opts(DetectionSize::Px(640)), &mut |_| {});
            record_pair(
                &report,
                false,
                gallery,
                &mut impostor_cosines,
                &mut false_positive,
                &mut true_negative,
                &mut impostor_not_compared,
                &mut impostor_undetected,
                &mut impostor_wrong_identity,
            );
        }
    }
    let (plate_match_candidate, plate_match_trials) = plate_trials(bundle, "FIX123", "FIX123");
    let (plate_mismatch_candidate, plate_mismatch_trials) = plate_trials(bundle, "OTHER1", "FIX123");
    let (plate_unpublished_candidate, plate_unpublished_trials) = plate_trials(bundle, "FIX123", "");

    let (c_hit, c_miss) = miss_rate(bundle, 30, true);
    let (m_hit, m_miss) = miss_rate(bundle, 30, false);
    let (bc_hit, bc_miss) = miss_rate(bundle, 3, true);
    let (bm_hit, bm_miss) = miss_rate(bundle, 3, false);

    Measurement {
        schema: "openworld.measurement.v1".into(),
        bundle_id: bundle.id.clone(),
        bundle_name: bundle.name.clone(),
        threshold: bundle.threshold,
        hardware: "cpu".into(),
        preprocess: "integer-nearest-long-side".into(),
        perception: PERCEPTION_FIDUCIAL.into(),
        detector_declared: bundle.detector.clone(),
        embedder_declared: bundle.embedder.clone(),
        genuine_cosines,
        impostor_cosines,
        true_positive,
        false_negative,
        false_positive,
        true_negative,
        genuine_not_compared,
        impostor_not_compared,
        genuine_undetected,
        impostor_undetected,
        genuine_wrong_identity,
        impostor_wrong_identity,
        plate_match_candidate,
        plate_match_trials,
        plate_mismatch_candidate,
        plate_mismatch_trials,
        plate_unpublished_candidate,
        plate_unpublished_trials,
        detection_hit,
        detection_miss,
        below_64_kept,
        faces_seen_not_compared,
        miss_1s_complete_hit: c_hit,
        miss_1s_complete_miss: c_miss,
        miss_1s_measured_hit: m_hit,
        miss_1s_measured_miss: m_miss,
        miss_brief_complete_hit: bc_hit,
        miss_brief_complete_miss: bc_miss,
        miss_brief_measured_hit: bm_hit,
        miss_brief_measured_miss: bm_miss,
    }
}

fn miss_rate(bundle: &Bundle, visible: u64, complete: bool) -> (u32, u32) {
    let mut hit = 0;
    let mut miss = 0;
    let phases = [0u64, 1, 2, 4, 5, 7, 9, 11];
    for start in phases {
        let mut frames = Vec::new();
        for i in 0..90 {
            let mut canvas = blank(200, 150);
            if i >= start && i < start + visible {
                let marker = fiducial::render_face_module(7, 16);
                fiducial::place(&mut canvas, &marker, 16, 12);
            }
            frames.push(canvas);
        }
        let mut opts = still_opts(DetectionSize::Full);
        opts.fps = 30.0;
        opts.coverage = if complete { Coverage::Complete } else { Coverage::Measured };
        let report = scan_images(&frames, bundle, &pack_with(7), &opts, &mut |_| {});
        if report.inventory.iter().any(|item| item.kind == "face") {
            hit += 1;
        } else {
            miss += 1;
        }
    }
    (hit, miss)
}

fn still_opts(detection: DetectionSize) -> ScanOpts {
    ScanOpts {
        detection,
        coverage: Coverage::Complete,
        execution: Execution::Cpu,
        form_factor: FormFactor::Computer,
        missing: true,
        wanted: true,
        abort_after_frames: None,
        fps: 0.0,
        frame_count_hint: None,
        warnings: Vec::new(),
        out_dir: None,
    }
}

fn pack_with(id: u16) -> PosterPack {
    PosterPack {
        schema: SCHEMA.into(),
        id: "measure".into(),
        source: "fixture".into(),
        perception: PERCEPTION_FIDUCIAL.into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        expires_at: "2027-12-31T00:00:00Z".into(),
        body_sha256: String::new(),
        posters: vec![Poster {
            id: format!("poster-{id}"),
            class: PosterClass::Missing,
            title: "Fixture subject".into(),
            fbi_url: "https://www.fbi.gov/wanted".into(),
            embedding: Some(unit_embedding(id)),
            fiducial_id: Some(id),
            plate: None,
            expires_at: None,
        }],
    }
}

fn face_at(id: u16, module: u32, x: u32, y: u32, w: u32, h: u32) -> RgbImage {
    let mut canvas = blank(w, h);
    let marker = fiducial::render_face_module(id, module);
    fiducial::place(&mut canvas, &marker, x, y);
    canvas
}

const COMPARISON_PLACES: [(u32, u32); 10] = [
    (8, 8),
    (24, 16),
    (40, 24),
    (56, 8),
    (72, 32),
    (16, 40),
    (48, 48),
    (80, 20),
    (32, 56),
    (64, 36),
];

fn comparison_places() -> [(u32, u32); 10] {
    COMPARISON_PLACES
}

fn record_pair(
    report: &crate::scan::ScanReport,
    genuine: bool,
    gallery: u16,
    cosines: &mut Vec<f32>,
    pass: &mut u32,
    fail: &mut u32,
    not_compared: &mut u32,
    undetected: &mut u32,
    wrong: &mut u32,
) {
    if report.comparisons.is_empty() {
        if report.faces_seen_not_compared > 0 {
            *not_compared += 1;
        } else {
            *undetected += 1;
        }
        return;
    }
    for cmp in &report.comparisons {
        let same = cmp.poster_fiducial_id == Some(cmp.fiducial_id);
        if cmp.poster_fiducial_id != Some(gallery) || same != genuine {
            *wrong += 1;
            continue;
        }
        cosines.push(cmp.cosine);
        if cmp.passed {
            *pass += 1;
        } else {
            *fail += 1;
        }
    }
}

fn plate_trials(bundle: &Bundle, seen: &str, published: &str) -> (u32, u32) {
    let mut candidates = 0u32;
    let trials = 40u32;
    for n in 0..trials {
        let mut canvas = blank(320, 200);
        if let Some(marker) = fiducial::render_plate(seen, 8) {
            fiducial::place(&mut canvas, &marker, 16 + n, 24);
        }
        let pack = if published.is_empty() { pack_with(1) } else { pack_with_plate(published) };
        let report = scan_images(&[canvas], bundle, &pack, &still_opts(DetectionSize::Px(640)), &mut |_| {});
        if report.candidates.iter().any(|candidate| candidate.kind == "plate") {
            candidates += 1;
        }
    }
    (candidates, trials)
}

fn pack_with_plate(plate: &str) -> PosterPack {
    PosterPack {
        schema: SCHEMA.into(),
        id: "measure-plate".into(),
        source: "fixture".into(),
        perception: PERCEPTION_FIDUCIAL.into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        expires_at: "2027-12-31T00:00:00Z".into(),
        body_sha256: String::new(),
        posters: vec![Poster {
            id: format!("plate-{plate}"),
            class: PosterClass::Wanted,
            title: "Fixture plate".into(),
            fbi_url: "https://www.fbi.gov/wanted".into(),
            embedding: None,
            fiducial_id: None,
            plate: Some(plate.to_string()),
            expires_at: None,
        }],
    }
}
