// SPDX-License-Identifier: Apache-2.0

//! A self-test of the scan pipeline on demo markers. Not a face measurement.
//!
//! It checks that the engine's rules hold end to end: a marker under the
//! compare size is seen and not compared, a matching marker becomes a
//! candidate, a marker for no poster does not, a face on screen for one video
//! frame is too brief to be a candidate, and `measured` coverage can miss a
//! face that `complete` reads. The counts are exact, so a change to the rules
//! shows up as a changed count. A bundle's curve comes from `eval/`, not here.

use crate::bundle::Bundle;
use crate::copy::Line;
use crate::engine::{DecodeEnd, Engine, EngineConfig, StopFlag};
use crate::estimate::Coverage;
use crate::fixture::{self, Markers, FIXTURE_CUTOFF};
use crate::posters::PosterPack;
use crate::report::{ScanReport, TrackStatus};
use image::RgbImage;
use serde::Serialize;
use std::time::SystemTime;

#[derive(Clone, Debug, Default, Serialize)]
pub struct SelfTest {
    pub schema: String,
    pub cutoff: f32,
    pub genuine_trials: u32,
    pub genuine_candidates: u32,
    pub impostor_trials: u32,
    pub impostor_candidates: u32,
    pub small_seen: u32,
    pub small_compared: u32,
    pub one_frame_video_candidates: u32,
    pub brief_complete_found: u32,
    pub brief_measured_found: u32,
    pub second_complete_found: u32,
    pub second_measured_found: u32,
}

fn scan(bundle: &Bundle, pack: &PosterPack, frames: &[RgbImage], coverage: Coverage, fps: f64) -> ScanReport {
    let cfg = EngineConfig {
        coverage,
        missing: true,
        wanted: true,
        fps,
        video: frames.len() > 1,
        on_screen: None,
        cover_index: None,
        warnings: Vec::<Line>::new(),
        out_dir: None,
        stop: StopFlag::default(),
        now: SystemTime::now(),
    };
    let mut engine = Engine::new(bundle.clone(), pack.clone(), cfg, Box::new(Markers)).expect("demo vectors are valid");
    for (index, frame) in frames.iter().enumerate() {
        engine.push(index as u64, frame, &mut |_| {});
    }
    engine.finish(DecodeEnd { clean: true, declared_frames: Some(frames.len() as u64), reason: None }, &mut |_| {})
}

fn still(id: u16, module: u32, x: u32, y: u32) -> RgbImage {
    let mut image = crate::geom::blank(400, 320);
    fixture::place(&mut image, &fixture::render(id, module), x, y);
    image
}

/// A clip of `frames` blank frames at 30 fps with the marker on frames `on`.
fn clip(id: u16, frames: u32, on: std::ops::Range<u32>) -> Vec<RgbImage> {
    (0..frames)
        .map(|n| if on.contains(&n) { still(id, fixture::SCENE_MODULE, 40, 40) } else { crate::geom::blank(400, 320) })
        .collect()
}

pub fn self_test(bundle: &Bundle, pack: &PosterPack) -> SelfTest {
    let mut out = SelfTest { schema: "openworld.selftest.v1".into(), cutoff: FIXTURE_CUTOFF, ..Default::default() };
    let posters: Vec<u16> = vec![7, 11];
    let found = |report: &ScanReport| !report.candidates.is_empty();
    for (k, id) in posters.iter().cycle().take(40).enumerate() {
        let (x, y) = (8 + (k as u32 * 7) % 200, 8 + (k as u32 * 5) % 150);
        out.genuine_trials += 1;
        out.genuine_candidates += u32::from(found(&scan(bundle, pack, &[still(*id, fixture::SCENE_MODULE, x, y)], Coverage::Complete, 0.0)));
        out.impostor_trials += 1;
        out.impostor_candidates += u32::from(found(&scan(bundle, pack, &[still(300 + k as u16, fixture::SCENE_MODULE, x, y)], Coverage::Complete, 0.0)));
    }
    for k in 0..10u32 {
        let report = scan(bundle, pack, &[still(7, 8, 20 + k * 9, 30)], Coverage::Complete, 0.0);
        out.small_seen += report.tracks.len() as u32;
        out.small_compared += report.tracks.iter().filter(|t| t.status != TrackStatus::NotCompared).count() as u32;
    }
    out.one_frame_video_candidates = u32::from(found(&scan(bundle, pack, &clip(7, 30, 12..13), Coverage::Complete, 30.0)));
    // Two frames at 30 fps: seen, under three quality frames and under a second.
    out.brief_complete_found = u32::from(scan(bundle, pack, &clip(7, 30, 13..15), Coverage::Complete, 30.0).tracks.iter().any(|t| t.status == TrackStatus::TooBrief));
    out.brief_measured_found = u32::from(!scan(bundle, pack, &clip(7, 30, 13..15), Coverage::Measured, 30.0).tracks.is_empty());
    out.second_complete_found = u32::from(found(&scan(bundle, pack, &clip(7, 60, 10..40), Coverage::Complete, 30.0)));
    out.second_measured_found = u32::from(found(&scan(bundle, pack, &clip(7, 60, 10..40), Coverage::Measured, 30.0)));
    out
}
