// SPDX-License-Identifier: Apache-2.0

//! The scan engine. Frames go in one at a time, in order; a report comes out.
//!
//! Every analyzed frame is read by the detector. Faces are followed by the
//! tracker. Each track keeps its few best frames that pass the quality gates;
//! when the face leaves, those frames are embedded once, averaged into one
//! template, and compared to the posters. So a face on screen for a minute
//! costs a handful of embeddings, and the result holds one row per face.
//!
//! A video face is a candidate only when its template passes the locked cutoff
//! and it passed the gates on at least three frames or across one second.
//! A still is one frame, and its one face decides.
//!
//! Memory is bounded by the number of faces on screen at once, not by the
//! length of the file. Nothing is kept per frame.

use crate::bundle::Bundle;
use crate::copy::{self, Line};
use crate::estimate::Coverage;
use crate::outdir;
use crate::perception::{self, auto_long_side, Aligned, Detection, Perception};
use crate::posters::{Poster, PosterClass, PosterPack};
use crate::quality::Reject;
use crate::report::{
    self, BundleInfo, Candidate, Counts, PackInfo, PosterRef, ScanReport, Status, TrackItem, TrackStatus, WeightInfo,
};
use crate::timeutil::format_rfc3339;
use crate::track::{ByteTrack, TrackDet};
use image::imageops::{self, FilterType};
use image::RgbImage;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Frames embedded per track: the sharpest, most frontal, most confident ones.
pub const TOP_K: usize = 5;
/// Two kept frames of one track are at least this far apart, so they are not one moment.
const KEEP_APART_SECONDS: f64 = 0.2;
/// A video face must pass the gates on this many frames, or across `MIN_SECONDS`.
pub const MIN_QUALITY_FRAMES: u32 = 3;
pub const MIN_SECONDS: f64 = 1.0;
/// Evidence pictures are written no larger than this on the long side.
const EVIDENCE_LONG: u32 = 1600;
const CROP_LONG: u32 = 320;
/// Progress is reported every this many analyzed frames.
const PROGRESS_EVERY: u64 = 15;

/// Why a scan stopped early. Set from any thread; read between frames.
#[derive(Clone, Default)]
pub struct StopFlag(Arc<AtomicU8>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Cancel = 1,
    Heat = 2,
    Battery = 3,
    Background = 4,
}

impl StopFlag {
    pub fn stop(&self, reason: StopReason) {
        let _ = self.0.compare_exchange(0, reason as u8, Ordering::SeqCst, Ordering::SeqCst);
    }

    pub fn reason(&self) -> Option<StopReason> {
        match self.0.load(Ordering::SeqCst) {
            1 => Some(StopReason::Cancel),
            2 => Some(StopReason::Heat),
            3 => Some(StopReason::Battery),
            4 => Some(StopReason::Background),
            _ => None,
        }
    }

    pub fn parse(code: &str) -> StopReason {
        match code {
            "heat" => StopReason::Heat,
            "battery" => StopReason::Battery,
            "background" => StopReason::Background,
            _ => StopReason::Cancel,
        }
    }
}

impl StopReason {
    fn line(self) -> Line {
        Line::of(match self {
            StopReason::Cancel => copy::CANCELLED,
            StopReason::Heat => copy::STOPPED_HEAT,
            StopReason::Battery => copy::STOPPED_BATTERY,
            StopReason::Background => copy::STOPPED_BACKGROUND,
        })
    }
}

/// What a scan sends while it runs.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Progress { frames_analyzed: u64, frame: u64 },
    /// A new face. `crop` is a first look; the track's row replaces it.
    Face { track_id: u64, frame: u64, frame_label: Line, crop: Option<String> },
    /// A face left the picture and was decided.
    Track { track: TrackItem },
}

pub struct EngineConfig {
    pub coverage: Coverage,
    pub missing: bool,
    pub wanted: bool,
    /// Frames a second. Zero for a still.
    pub fps: f64,
    pub video: bool,
    /// Frame indexes that `measured` keeps, when they are not the even rate.
    pub on_screen: Option<Vec<u64>>,
    /// A motion photo's cover. Measured coverage always reads it.
    pub cover_index: Option<u64>,
    pub warnings: Vec<Line>,
    pub out_dir: Option<PathBuf>,
    pub stop: StopFlag,
    pub now: SystemTime,
}

/// How decoding ended, from the decoder that fed the frames.
#[derive(Clone, Debug, Default)]
pub struct DecodeEnd {
    /// The decoder reached the end of the pictures without an error.
    pub clean: bool,
    /// The picture count the decoder promised. A different count pushed is incomplete.
    pub declared_frames: Option<u64>,
    pub reason: Option<Line>,
}

struct Entry {
    t: f64,
    rank: f32,
    aligned: Aligned,
    crop: RgbImage,
}

struct TrackState {
    first: u64,
    last: u64,
    face_px: u32,
    quality_frames: u32,
    quality_first_t: f64,
    quality_last_t: f64,
    rejects: [u32; 6],
    top: Vec<Entry>,
    /// The best frame, encoded, for the evidence picture of a candidate.
    best_frame: Option<(u64, Vec<u8>)>,
    first_crop: Option<String>,
}

struct Prepared<'a> {
    poster: &'a Poster,
    vector: &'a [f32],
}

pub struct Engine {
    bundle: Bundle,
    pack: PosterPack,
    cfg: EngineConfig,
    perception: Box<dyn Perception + Send>,
    cutoff: f32,
    tracker: ByteTrack,
    open: BTreeMap<u64, TrackState>,
    done: Vec<TrackItem>,
    candidates: Vec<Candidate>,
    frames_pushed: u64,
    frames_analyzed: u64,
    next_tick: u64,
    det_long: u32,
    original_long: u32,
    stopped: Option<Line>,
    failure: Option<Line>,
    templates: u64,
}

impl Engine {
    /// The pack's vectors are checked against this perception before any frame.
    pub fn new(bundle: Bundle, pack: PosterPack, cfg: EngineConfig, perception: Box<dyn Perception + Send>) -> Result<Self, Line> {
        pack.check_vectors(perception.dim())?;
        let cutoff = perception.cutoff(bundle.threshold);
        let tracker = ByteTrack::new(perception.high_score());
        Ok(Self {
            bundle,
            pack,
            cfg,
            perception,
            cutoff,
            tracker,
            open: BTreeMap::new(),
            done: Vec::new(),
            candidates: Vec::new(),
            frames_pushed: 0,
            frames_analyzed: 0,
            next_tick: 0,
            det_long: 0,
            original_long: 0,
            stopped: None,
            failure: None,
            templates: 0,
        })
    }

    pub fn set_stop(&mut self, stop: StopFlag) {
        self.cfg.stop = stop;
    }

    /// Measured coverage reads these frames: the pictures on screen at each tick.
    pub fn set_on_screen(&mut self, frames: Option<Vec<u64>>) {
        self.cfg.on_screen = frames;
    }

    /// A motion photo's cover is read whatever the coverage.
    pub fn set_cover(&mut self, index: u64) {
        self.cfg.cover_index = Some(index);
    }

    /// Whether the next frame, `index`, will be analyzed. A phone asks before
    /// converting a frame, so `measured` coverage does not copy frames it skips.
    pub fn wants(&mut self, index: u64) -> bool {
        if self.cfg.cover_index == Some(index) || self.cfg.coverage != Coverage::Measured {
            return true;
        }
        if let Some(keep) = &self.cfg.on_screen {
            return keep.binary_search(&index).is_ok();
        }
        let mut tick = self.next_tick;
        crate::geom::take_measured_sample(index, &mut tick, self.cfg.fps)
    }

    /// Push frame `index`. Frames arrive in order, every decoded frame, whether
    /// or not it is analyzed. Returns false when the scan has stopped.
    pub fn push(&mut self, index: u64, frame: &RgbImage, events: &mut dyn FnMut(Event)) -> bool {
        if self.stopped.is_some() || self.failure.is_some() {
            return false;
        }
        if let Some(reason) = self.cfg.stop.reason() {
            self.stopped = Some(reason.line());
            return false;
        }
        if index != self.frames_pushed {
            self.failure = Some(Line::of(copy::NOT_FULLY_DECODED));
            return false;
        }
        self.frames_pushed += 1;
        if !self.take_sample(index) {
            return true;
        }
        if self.det_long == 0 {
            self.original_long = frame.width().max(frame.height());
            self.det_long = auto_long_side(self.original_long, self.perception.min_face_px());
        }
        let detections = match self.perception.detect(frame, self.det_long) {
            Ok(found) => found,
            Err(message) => {
                self.failure = Some(runtime_line(&message));
                return false;
            }
        };
        let t = self.time_of(index);
        let dets: Vec<TrackDet> = detections.iter().map(|d| TrackDet { rect: d.rect, score: d.score }).collect();
        let update = self.tracker.update(&dets, t);
        for (det, id) in detections.iter().zip(&update.ids) {
            if let Err(message) = self.observe(*id, index, t, frame, det, events) {
                self.failure = Some(runtime_line(&message));
                return false;
            }
        }
        for id in update.ended {
            if let Err(message) = self.close(id, events) {
                self.failure = Some(runtime_line(&message));
                return false;
            }
        }
        self.frames_analyzed += 1;
        if self.frames_analyzed % PROGRESS_EVERY == 0 {
            events(Event::Progress { frames_analyzed: self.frames_analyzed, frame: index });
        }
        true
    }

    /// Frame `index` was decoded and is not analyzed: `wants` said no. Nothing is copied.
    pub fn skip(&mut self, index: u64) -> bool {
        if index != self.frames_pushed || self.take_sample(index) {
            self.failure.get_or_insert(Line::of(copy::NOT_FULLY_DECODED));
            return false;
        }
        self.frames_pushed += 1;
        true
    }

    fn take_sample(&mut self, index: u64) -> bool {
        if self.cfg.cover_index == Some(index) || self.cfg.coverage != Coverage::Measured {
            return true;
        }
        if let Some(keep) = &self.cfg.on_screen {
            return keep.binary_search(&index).is_ok();
        }
        crate::geom::take_measured_sample(index, &mut self.next_tick, self.cfg.fps)
    }

    fn time_of(&self, index: u64) -> f64 {
        if self.cfg.fps > 0.0 && self.cfg.fps.is_finite() {
            index as f64 / self.cfg.fps
        } else {
            index as f64
        }
    }

    fn observe(&mut self, id: u64, index: u64, t: f64, frame: &RgbImage, det: &Detection, events: &mut dyn FnMut(Event)) -> Result<(), String> {
        let new = !self.open.contains_key(&id);
        if new {
            let crop = self.write_crop(&format!("crops/face_{id}.jpg"), &crop_of(frame, det));
            events(Event::Face { track_id: id, frame: index, frame_label: copy::frame_label(index), crop: crop.clone() });
            self.open.insert(
                id,
                TrackState {
                    first: index,
                    last: index,
                    face_px: 0,
                    quality_frames: 0,
                    quality_first_t: t,
                    quality_last_t: t,
                    rejects: [0; 6],
                    top: Vec::new(),
                    best_frame: None,
                    first_crop: crop,
                },
            );
        }
        let assessed = self.perception.assess(frame, det);
        let state = self.open.get_mut(&id).expect("track was just opened");
        state.last = index;
        state.face_px = state.face_px.max(det.rect.short());
        let measured = match assessed {
            Ok(measured) => measured,
            Err(reject) => {
                state.rejects[reject_slot(reject)] += 1;
                return Ok(());
            }
        };
        if state.quality_frames == 0 {
            state.quality_first_t = t;
        }
        state.quality_frames += 1;
        state.quality_last_t = t;
        let Some(slot) = keep_slot(&state.top, t, measured.rank) else {
            return Ok(());
        };
        let Some(aligned) = self.perception.align(frame, det, index)? else {
            let state = self.open.get_mut(&id).expect("track is open");
            state.rejects[reject_slot(Reject::Unaligned)] += 1;
            return Ok(());
        };
        let state = self.open.get_mut(&id).expect("track is open");
        let entry = Entry { t, rank: measured.rank, aligned, crop: crop_of(frame, det) };
        match slot {
            Slot::Push => state.top.push(entry),
            Slot::Replace(i) => state.top[i] = entry,
        }
        let best = state.top.iter().map(|e| e.rank).fold(f32::MIN, f32::max);
        if measured.rank >= best {
            state.best_frame = Some((index, encode_jpeg(&shrink(frame, EVIDENCE_LONG), 88)));
        }
        Ok(())
    }

    /// Embed a track's kept frames once, average them, compare, and decide.
    fn close(&mut self, id: u64, events: &mut dyn FnMut(Event)) -> Result<(), String> {
        let Some(state) = self.open.remove(&id) else {
            return Ok(());
        };
        let frames = copy::frames_span(state.first, state.last);
        let best = state.top.iter().max_by(|a, b| a.rank.total_cmp(&b.rank));
        let crop = match best {
            Some(entry) => self.write_crop(&format!("crops/face_{id}.jpg"), &entry.crop).or(state.first_crop.clone()),
            None => state.first_crop.clone(),
        };
        let mut item = TrackItem {
            track_id: id,
            first_frame: state.first,
            last_frame: state.last,
            frames,
            face_px: state.face_px,
            quality_frames: state.quality_frames,
            status: TrackStatus::NotCompared,
            label: Line::of(copy::NOT_COMPARED),
            reason: None,
            crop: crop.clone(),
            scores: Vec::new(),
        };
        if state.top.is_empty() {
            item.reason = Some(Line::of(main_reject(&state.rejects).copy()));
            return self.finish_track(item, events);
        }
        let mut vectors = Vec::new();
        for entry in &state.top {
            let v = self.perception.embed(&entry.aligned)?;
            if perception::valid_vector(&v, self.perception.dim()) {
                vectors.push(v);
            }
        }
        let Some(template) = mean_unit(&vectors) else {
            item.status = TrackStatus::Unscored;
            item.label = Line::of(copy::FACE_UNSCORED);
            return self.finish_track(item, events);
        };
        self.templates += 1;
        let posters = self.prepared();
        let mut passing: Vec<(&Poster, f32)> = Vec::new();
        for prepared in &posters {
            let Some(score) = perception::cosine(&template, prepared.vector) else {
                continue;
            };
            if cfg!(feature = "eval") {
                item.scores.push(report::PosterScore { poster_id: prepared.poster.id.clone(), score });
            }
            if score >= self.cutoff {
                passing.push((prepared.poster, score));
            }
        }
        passing.sort_by(|a, b| b.1.total_cmp(&a.1));
        let brief = self.cfg.video
            && state.quality_frames < MIN_QUALITY_FRAMES
            && state.quality_last_t - state.quality_first_t < MIN_SECONDS;
        if passing.is_empty() {
            item.status = TrackStatus::BelowCutoff;
            item.label = Line::of(copy::BELOW_CUTOFF);
        } else if brief {
            item.status = TrackStatus::TooBrief;
            item.label = Line::of(copy::TOO_BRIEF);
        } else {
            item.status = TrackStatus::Candidate;
            item.label = Line::of(copy::POSSIBLE_CANDIDATE);
            let (frame_index, frame) = match &state.best_frame {
                Some((index, bytes)) => (*index, self.write_bytes(&format!("frames/frame_{index}.jpg"), bytes)),
                None => (state.first, None),
            };
            let cards: Vec<Candidate> = passing
                .iter()
                .map(|(poster, score)| self.card(id, frame_index, poster, *score, crop.clone(), frame.clone()))
                .collect();
            self.candidates.extend(cards);
        }
        self.finish_track(item, events)
    }

    fn finish_track(&mut self, item: TrackItem, events: &mut dyn FnMut(Event)) -> Result<(), String> {
        events(Event::Track { track: item.clone() });
        self.done.push(item);
        Ok(())
    }

    fn prepared(&self) -> Vec<Prepared<'_>> {
        self.pack
            .enabled(self.cfg.missing, self.cfg.wanted)
            .into_iter()
            .filter_map(|poster| Some(Prepared { poster, vector: poster.embedding.as_deref()? }))
            .collect()
    }

    fn card(&self, track_id: u64, frame_index: u64, poster: &Poster, score: f32, crop: Option<String>, frame: Option<String>) -> Candidate {
        let wanted = poster.class == PosterClass::Wanted;
        Candidate {
            headline: Line::of(copy::POSSIBLE_CANDIDATE),
            track_id,
            frame_index,
            frame_label: copy::frame_label(frame_index),
            band: band(score, self.cutoff),
            poster: PosterRef {
                id: poster.id.clone(),
                class: poster.class.as_str().into(),
                class_label: poster.class.label().into(),
                title: poster.title.clone(),
                fbi_url: poster.fbi_url.clone(),
                image: poster.image.as_ref().map(|name| self.pack.dir.join(name).to_string_lossy().to_string()),
            },
            crop,
            frame,
            leaving: Line::of(copy::LEAVING),
            leave_notes: copy::leave_notes(wanted, &self.cfg.warnings),
            #[cfg(feature = "eval")]
            score,
        }
    }

    fn write_crop(&self, name: &str, image: &RgbImage) -> Option<String> {
        self.write_bytes(name, &encode_jpeg(&shrink(image, CROP_LONG), 92))
    }

    fn write_bytes(&self, name: &str, bytes: &[u8]) -> Option<String> {
        let dir = self.cfg.out_dir.as_ref()?;
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, bytes).ok()?;
        Some(name.to_string())
    }

    pub fn frames_pushed(&self) -> u64 {
        self.frames_pushed
    }

    /// Close every open track and build the report. `end` says how decoding ended.
    pub fn finish(self, end: DecodeEnd, events: &mut dyn FnMut(Event)) -> ScanReport {
        self.finish_keeping(end, events).0
    }

    /// `finish`, handing back the networks so the next scan need not load them.
    pub fn finish_keeping(mut self, end: DecodeEnd, events: &mut dyn FnMut(Event)) -> (ScanReport, Box<dyn Perception + Send>) {
        for id in self.tracker.close_all() {
            if let Err(message) = self.close(id, events) {
                self.failure.get_or_insert(runtime_line(&message));
                break;
            }
        }
        let mut reason = self.failure.clone().or(self.stopped.clone());
        if reason.is_none() && !end.clean {
            reason = Some(end.reason.clone().unwrap_or_else(|| Line::of(copy::NOT_FULLY_DECODED)));
        }
        if reason.is_none() && end.declared_frames.is_some_and(|declared| declared != self.frames_pushed) {
            reason = Some(Line::of(copy::NOT_FULLY_DECODED));
        }
        if reason.is_none() && self.frames_pushed == 0 {
            reason = Some(Line::of(copy::NOT_FULLY_DECODED));
        }
        let mut tracks = std::mem::take(&mut self.done);
        tracks.sort_by_key(|t| (t.first_frame, t.track_id));
        let compared = tracks.iter().filter(|t| matches!(t.status, TrackStatus::Candidate | TrackStatus::BelowCutoff | TrackStatus::TooBrief)).count() as u64;
        let seen = tracks.len() as u64;
        let candidates = std::mem::take(&mut self.candidates);
        let complete = reason.is_none();
        let headline = if !complete {
            Line::of(copy::INCOMPLETE)
        } else if candidates.is_empty() {
            Line::of(copy::NO_CLEARANCE)
        } else {
            Line::of(copy::POSSIBLE_CANDIDATE)
        };
        let mut warnings = self.cfg.warnings.clone();
        if self.pack.stale(self.cfg.now) {
            warnings.push(Line::of(copy::POSTERS_STALE));
        }
        let execution = crate::runtime::execution(self.perception.provider());
        let posters_on = self.pack.enabled(self.cfg.missing, self.cfg.wanted).len();
        let created = format_rfc3339(self.cfg.now);
        let expires = result_expiry(self.cfg.now, &self.pack.expires_at);
        let mut report = ScanReport {
            schema: report::RESULT_SCHEMA.into(),
            status: if complete { Status::Complete } else { Status::Incomplete },
            headline,
            reason,
            refusal: None,
            bundle: bundle_info(&self.bundle),
            posters: pack_info(&self.pack),
            perception: self.perception.kind().into(),
            perception_note: self.pack.demo().then(|| Line::of(copy::FIXTURE_MARKERS)),
            coverage: self.cfg.coverage.as_str().into(),
            coverage_line: Some(self.cfg.coverage.line()),
            coverage_banner: (self.cfg.coverage == Coverage::Measured).then(|| Line::of(copy::BRIEF_FACE)),
            detection_long_side: self.det_long,
            detection_line: (self.det_long > 0).then(|| copy::detection_line(self.det_long, self.original_long)),
            provider: self.perception.provider().into(),
            execution: execution.as_str().into(),
            device_note: execution.device_note().map(Line::of),
            classes: Some(Line::of(copy::class_note(self.cfg.missing, self.cfg.wanted))),
            frames_line: copy::frames_note(self.frames_analyzed, self.frames_pushed),
            faces_line: Some(copy::faces_line(seen, compared)),
            chance: self.chance(posters_on),
            warnings,
            disclosure: report::disclosure(complete),
            counts: Counts {
                frames_decoded: self.frames_pushed,
                frames_analyzed: self.frames_analyzed,
                frames_declared: end.declared_frames,
                faces_seen: seen,
                faces_compared: compared,
                faces_not_compared: seen - compared,
                candidates: candidates.len() as u64,
            },
            tracks,
            candidates,
            created_at: created,
            expires_at: expires,
        };
        if let Some(dir) = &self.cfg.out_dir {
            let written = serde_json::to_vec_pretty(&report)
                .ok()
                .is_some_and(|bytes| std::fs::write(dir.join(outdir::RESULT_FILE), bytes).is_ok());
            if !written {
                report.mark_incomplete(Line::of(copy::RESULT_UNWRITTEN));
            }
        }
        (report, self.perception)
    }

    /// How many false candidates chance alone would give this scan.
    fn chance(&self, posters: usize) -> Option<Line> {
        if self.pack.demo() {
            return None;
        }
        let Some(curve) = self.bundle.curve.as_ref().filter(|c| c.fmr.is_some()) else {
            return Some(Line::of(copy::CHANCE_UNKNOWN));
        };
        let fmr = curve.fmr.unwrap_or(0.0);
        let expected = fmr * self.templates as f64 * posters as f64;
        Some(copy::chance_line(expected, self.templates, posters, &curve.fmr_source))
    }
}

enum Slot {
    Push,
    Replace(usize),
}

/// Where a frame of this rank goes among a track's kept frames, if anywhere.
/// A frame close in time to a kept one competes with it, not with the rest.
fn keep_slot(top: &[Entry], t: f64, rank: f32) -> Option<Slot> {
    if let Some(i) = top.iter().position(|e| (e.t - t).abs() < KEEP_APART_SECONDS) {
        return (rank > top[i].rank).then_some(Slot::Replace(i));
    }
    if top.len() < TOP_K {
        return Some(Slot::Push);
    }
    let (worst, entry) = top.iter().enumerate().min_by(|a, b| a.1.rank.total_cmp(&b.1.rank))?;
    (rank > entry.rank).then_some(Slot::Replace(worst))
}

/// The unit mean of unit vectors. `None` when there are none or they cancel.
pub fn mean_unit(vectors: &[Vec<f32>]) -> Option<Vec<f32>> {
    let first = vectors.first()?;
    let mut sum = vec![0f32; first.len()];
    for v in vectors {
        for (s, x) in sum.iter_mut().zip(v) {
            *s += x;
        }
    }
    perception::normalize(&mut sum).then_some(sum)
}

/// How far past the cutoff, in words. The number itself is not shown.
pub fn band(score: f32, cutoff: f32) -> Line {
    let margin = score - cutoff;
    Line::of(if margin < 0.04 {
        copy::BAND_NEAR
    } else if margin < 0.12 {
        copy::BAND_ABOVE
    } else {
        copy::BAND_WELL
    })
}

fn reject_slot(reject: Reject) -> usize {
    match reject {
        Reject::Small => 0,
        Reject::Unaligned => 1,
        Reject::Confidence => 2,
        Reject::Pose => 3,
        Reject::Blur => 4,
        Reject::Exposure => 5,
    }
}

fn main_reject(counts: &[u32; 6]) -> Reject {
    const ORDER: [Reject; 6] = [Reject::Small, Reject::Unaligned, Reject::Confidence, Reject::Pose, Reject::Blur, Reject::Exposure];
    let (slot, _) = counts.iter().enumerate().max_by_key(|(i, n)| (**n, std::cmp::Reverse(*i))).unwrap_or((0, &0));
    ORDER[slot]
}

fn runtime_line(message: &str) -> Line {
    Line::with("runtime_failed", format!("The scan stopped: {message}"), &[])
}

/// The face box with a margin, for the person looking at it.
fn crop_of(frame: &RgbImage, det: &Detection) -> RgbImage {
    let r = det.rect;
    let pad_x = r.w / 4;
    let pad_y = r.h / 4;
    let rect = crate::geom::Rect { x: r.x.saturating_sub(pad_x), y: r.y.saturating_sub(pad_y), w: r.w + 2 * pad_x, h: r.h + 2 * pad_y };
    crate::geom::crop(frame, rect).unwrap_or_else(|| RgbImage::new(1, 1))
}

fn shrink(image: &RgbImage, long: u32) -> RgbImage {
    let (w, h) = image.dimensions();
    if w.max(h) <= long {
        return image.clone();
    }
    let scale = long as f32 / w.max(h) as f32;
    imageops::resize(image, ((w as f32 * scale) as u32).max(1), ((h as f32 * scale) as u32).max(1), FilterType::Triangle)
}

fn encode_jpeg(image: &RgbImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality).encode_image(image);
    out
}

fn result_expiry(now: SystemTime, pack_expires: &str) -> String {
    let week = now + Duration::from_secs(outdir::RESULT_DAYS * 86400);
    let pack = crate::timeutil::parse_rfc3339(pack_expires).unwrap_or(week);
    format_rfc3339(week.min(pack))
}

pub fn bundle_info(bundle: &Bundle) -> BundleInfo {
    BundleInfo {
        id: bundle.id.clone(),
        name: bundle.name.clone(),
        version: bundle.version.clone(),
        measured: bundle.curve_exists(),
        weights: bundle
            .weights
            .iter()
            .filter(|w| w.present)
            .map(|w| WeightInfo { role: w.role.clone(), name: w.name.clone(), sha256: w.sha256.clone() })
            .collect(),
    }
}

pub fn pack_info(pack: &PosterPack) -> PackInfo {
    PackInfo {
        id: pack.id.clone(),
        source: pack.source.clone(),
        sha256: pack.body_sha256.clone(),
        created_at: pack.created_at.clone(),
        expires_at: pack.expires_at.clone(),
        missing: pack.count(PosterClass::Missing),
        wanted: pack.count(PosterClass::Wanted),
        demo: pack.demo(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(t: f64, rank: f32) -> Entry {
        Entry { t, rank, aligned: Aligned::Marker { id: 0, rect: crate::geom::Rect { x: 0, y: 0, w: 1, h: 1 }, frame: 0 }, crop: RgbImage::new(1, 1) }
    }

    #[test]
    fn a_track_keeps_its_best_frames_spread_in_time() {
        let mut top = Vec::new();
        for k in 0..TOP_K {
            assert!(matches!(keep_slot(&top, k as f64, 0.5), Some(Slot::Push)));
            top.push(entry(k as f64, 0.5));
        }
        // Full: a worse frame is dropped, a better one replaces the worst.
        assert!(keep_slot(&top, 10.0, 0.4).is_none());
        assert!(matches!(keep_slot(&top, 10.0, 0.6), Some(Slot::Replace(_))));
        // A frame a moment after a kept one competes only with that one.
        assert!(matches!(keep_slot(&top, 2.05, 0.9), Some(Slot::Replace(2))));
        assert!(keep_slot(&top, 2.05, 0.1).is_none());
    }

    #[test]
    fn a_template_is_the_unit_mean() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let m = mean_unit(&[a.clone(), b]).unwrap();
        assert!((m[0] - m[1]).abs() < 1e-6 && (m[0] * m[0] + m[1] * m[1] - 1.0).abs() < 1e-5);
        assert!(mean_unit(&[a.clone(), vec![-1.0, 0.0]]).is_none());
        assert!(mean_unit(&[]).is_none());
    }

    #[test]
    fn bands_name_the_margin_and_not_the_score() {
        assert_eq!(band(0.65, 0.63).code, "band_near");
        assert_eq!(band(0.70, 0.63).code, "band_above");
        assert_eq!(band(0.90, 0.63).code, "band_well");
    }

    #[test]
    fn the_commonest_reject_names_why() {
        assert_eq!(main_reject(&[0, 0, 0, 3, 1, 0]), Reject::Pose);
        assert_eq!(main_reject(&[2, 0, 0, 2, 0, 0]), Reject::Small);
        assert_eq!(main_reject(&[0; 6]), Reject::Small);
    }

    #[test]
    fn a_stop_keeps_the_first_reason() {
        let flag = StopFlag::default();
        assert_eq!(flag.reason(), None);
        flag.stop(StopReason::Heat);
        flag.stop(StopReason::Cancel);
        assert_eq!(flag.reason(), Some(StopReason::Heat));
        assert_eq!(StopFlag::parse("battery"), StopReason::Battery);
        assert_eq!(StopFlag::parse("anything"), StopReason::Cancel);
    }
}
