// SPDX-License-Identifier: Apache-2.0

//! What a scan returns, and what it writes to `result.json`.
//!
//! One row per face track, not per frame, so a long video gives a short
//! result. No raw score and no near-miss list leave the library: a candidate
//! carries a band, and a face under the cutoff carries only that it was under.

use crate::copy::{self, Copy, Line};
use serde::{Deserialize, Serialize};

pub const RESULT_SCHEMA: &str = "openworld.result.v2";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Complete,
    Incomplete,
    Refused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackStatus {
    Candidate,
    BelowCutoff,
    TooBrief,
    NotCompared,
    Unscored,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackItem {
    pub track_id: u64,
    pub first_frame: u64,
    pub last_frame: u64,
    pub frames: Line,
    /// The largest short side of this face in the original file, in pixels.
    pub face_px: u32,
    /// Frames of this face that passed every quality gate.
    pub quality_frames: u32,
    pub status: TrackStatus,
    pub label: Line,
    /// Why it was not compared, when it was not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<Line>,
    pub crop: Option<String>,
    /// Every poster score for this track. Only an `eval` build writes it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scores: Vec<PosterScore>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PosterScore {
    pub poster_id: String,
    pub score: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PosterRef {
    pub id: String,
    pub class: String,
    pub class_label: String,
    pub title: String,
    pub fbi_url: Option<String>,
    /// The poster photo, when the pack carries one beside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub headline: Line,
    pub track_id: u64,
    pub frame_index: u64,
    pub frame_label: Line,
    pub band: Line,
    pub poster: PosterRef,
    pub crop: Option<String>,
    pub frame: Option<String>,
    pub leaving: Line,
    pub leave_notes: Vec<Line>,
    #[cfg(feature = "eval")]
    pub score: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BundleInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub measured: bool,
    pub weights: Vec<WeightInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeightInfo {
    pub role: String,
    pub name: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PackInfo {
    pub id: String,
    pub source: String,
    pub sha256: String,
    pub created_at: String,
    pub expires_at: String,
    pub missing: usize,
    pub wanted: usize,
    pub demo: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Counts {
    pub frames_decoded: u64,
    pub frames_analyzed: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames_declared: Option<u64>,
    pub faces_seen: u64,
    pub faces_compared: u64,
    pub faces_not_compared: u64,
    pub candidates: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanReport {
    pub schema: String,
    pub status: Status,
    pub headline: Line,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    pub bundle: BundleInfo,
    pub posters: PackInfo,
    pub perception: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perception_note: Option<Line>,
    pub coverage: String,
    pub coverage_line: Option<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_banner: Option<Line>,
    pub detection_long_side: u32,
    pub detection_line: Option<Line>,
    pub provider: String,
    pub execution: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_note: Option<Line>,
    pub classes: Option<Line>,
    pub frames_line: Option<Line>,
    pub faces_line: Option<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chance: Option<Line>,
    pub warnings: Vec<Line>,
    pub disclosure: Vec<Line>,
    pub counts: Counts,
    pub tracks: Vec<TrackItem>,
    pub candidates: Vec<Candidate>,
    pub created_at: String,
    /// The result is deleted after this, or when its poster pack expires.
    pub expires_at: String,
}

impl ScanReport {
    pub fn is_refused(&self) -> bool {
        self.status == Status::Refused
    }

    /// Unfinished or refused: the clearance sentence comes off.
    pub fn mark_incomplete(&mut self, reason: Line) {
        self.status = Status::Incomplete;
        self.headline = Line::of(copy::INCOMPLETE);
        self.reason = Some(reason);
        self.disclosure = disclosure(false);
    }
}

/// A finished scan keeps the clearance sentence. An unfinished or refused one does not.
pub fn disclosure(include_clearance: bool) -> Vec<Line> {
    copy::DISCLOSURE
        .iter()
        .filter(|line| include_clearance || **line != copy::NO_CLEARANCE)
        .map(|line| Line::of(*line))
        .collect()
}

pub fn refused(copy: Copy) -> ScanReport {
    refused_line(Line::of(copy))
}

/// A refusal. Its headline is the refusal sentence. It names no bundle.
pub fn refused_line(line: Line) -> ScanReport {
    ScanReport {
        schema: RESULT_SCHEMA.into(),
        status: Status::Refused,
        refusal: Some(line.code.clone()),
        headline: line,
        reason: None,
        bundle: BundleInfo::default(),
        posters: PackInfo::default(),
        perception: String::new(),
        perception_note: None,
        coverage: String::new(),
        coverage_line: None,
        coverage_banner: None,
        detection_long_side: 0,
        detection_line: None,
        provider: String::new(),
        execution: String::new(),
        device_note: None,
        classes: None,
        frames_line: None,
        faces_line: None,
        chance: None,
        warnings: Vec::new(),
        disclosure: disclosure(false),
        counts: Counts::default(),
        tracks: Vec::new(),
        candidates: Vec::new(),
        created_at: String::new(),
        expires_at: String::new(),
    }
}
