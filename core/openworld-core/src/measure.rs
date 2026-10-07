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

    for n in 0..12 {
        let image = face_at(7, 8, 12 + n * 3, 10 + (n % 4) * 2, 240, 180);
        let report = scan_images(&[image], bundle, &pack_with(7), &still_opts(DetectionSize::Px(640)), &mut |_| {});
        if report.inventory.iter().any(|item| item.kind == "face") {
            detection_hit += 1;
        } else {
            detection_miss += 1;
        }
    }
    for n in 0..4 {
        let image = face_at(7, 4, 16 + n * 4, 16, 240, 180);
        let report = scan_images(&[image], bundle, &pack_with(7), &still_opts(DetectionSize::Full), &mut |_| {});
        if report.inventory.iter().any(|item| item.kind == "face") {
            below_64_kept += 1;
        }
    }
    for n in 0..4 {
        let image = face_at(7, 8, 20 + n * 5, 18, 240, 180);
        let report = scan_images(&[image], bundle, &pack_with(7), &still_opts(DetectionSize::Full), &mut |_| {});
        faces_seen_not_compared += report.faces_seen_not_compared as u32;
    }
    for n in 0..40 {
        let x = 8 + (n % 8) * 4;
        let y = 8 + (n / 8) * 5;
        let image = face_at(7, 16, x, y, 360, 280);
        let report = scan_images(&[image], bundle, &pack_with(7), &still_opts(DetectionSize::Px(640)), &mut |_| {});
        for cmp in report.comparisons {
            if cmp.poster_fiducial_id == Some(cmp.fiducial_id) {
                genuine_cosines.push(cmp.cosine);
            }
        }
    }
    for n in 0..80 {
        let x = 8 + (n % 10) * 4;
        let y = 8 + (n / 10) * 4;
        let image = face_at(99, 16, x, y, 360, 280);
        let report = scan_images(&[image], bundle, &pack_with(3), &still_opts(DetectionSize::Px(640)), &mut |_| {});
        for cmp in report.comparisons {
            impostor_cosines.push(cmp.cosine);
        }
    }

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
