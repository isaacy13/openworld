// SPDX-License-Identifier: Apache-2.0

use crate::bundle::Bundle;
use crate::copy::{
    class_note, frame_label, frames_note, BELOW_CUTOFF, BRIEF_FACE, DISCLOSURE, FACE_UNSCORED, FIXTURE_MARKERS, INCOMPLETE,
    NO_CLEARANCE, NOT_COMPARED, PLATE_NOT_ON_POSTER, PLATE_UNPUBLISHED, PLATE_UNREAD, POSSIBLE_CANDIDATE, VEHICLE_NOT_PERSON,
};
use crate::decode::{self, MediaError};
use crate::embed::fixture_probe;
use crate::estimate::{Coverage, DetectionSize, FormFactor};
use crate::fiducial::{self, Marker, PERCEPTION_FIDUCIAL, PERCEPTION_ONNX};
use crate::onnx_exec::FaceModels;
use crate::geom::{self, FrameMap, Rect, FACE_COMPARE_PX, FACE_SEEN_PX, PLATE_MIN_HEIGHT, PLATE_MIN_WIDTH};
use crate::hardware::Execution;
use crate::posters::{normalize_plate, PackError, Poster, PosterPack};
use crate::track::{ByteTrack, TrackClass, TrackDet};
use image::RgbImage;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InventoryItem {
    pub kind: String,
    pub frame_index: u64,
    pub frame_label: String,
    pub track_id: u64,
    pub det_short_px: u32,
    pub orig_short_px: u32,
    pub label: String,
    pub compared: bool,
    pub crop: Option<String>,
    pub fiducial_id: Option<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub wording: String,
    pub kind: String,
    pub frame_index: u64,
    pub frame_label: String,
    pub track_id: u64,
    pub cosine: Option<f32>,
    pub threshold: f32,
    pub uncertainty: String,
    pub poster_id: String,
    pub poster_class: String,
    pub poster_class_label: String,
    pub poster_title: String,
    pub fbi_url: String,
    pub plate: Option<String>,
    pub crop: Option<String>,
    pub frame: Option<String>,
    pub leaving: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comparison {
    pub frame_index: u64,
    pub poster_id: String,
    pub cosine: f32,
    pub passed: bool,
    pub fiducial_id: u16,
    pub poster_fiducial_id: Option<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanReport {
    pub status: String,
    pub refusal: Option<String>,
    pub message: String,
    pub summary: String,
    pub bundle_id: String,
    pub bundle_name: String,
    pub bundle_version: String,
    pub threshold: f32,
    pub detector_declared: String,
    pub embedder_declared: String,
    pub perception: String,
    pub perception_note: String,
    pub coverage: String,
    pub coverage_banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_note: Option<String>,
    pub detection: String,
    pub execution: String,
    pub device_note: Option<String>,
    pub warnings: Vec<String>,
    pub disclosure: Vec<String>,
    pub frames_decoded: u64,
    pub frames_analyzed: u64,
    pub faces_embedded: u64,
    pub plates_ocr_attempted: u64,
    pub faces_seen_not_compared: u64,
    pub inventory: Vec<InventoryItem>,
    pub candidates: Vec<Candidate>,
    pub comparisons: Vec<Comparison>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub frame_index: u64,
    pub frame_label: String,
    pub label: String,
    pub kind: String,
    pub crop: Option<String>,
}

/// Size and timing from AVFoundation or MediaCodec. When this is set, FFmpeg is not used.
#[derive(Clone, Debug)]
pub struct MediaFacts {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frames: u64,
    pub duration_sec: f64,
    pub video: bool,
    /// Container creation time, seconds since the Unix epoch, when the file has one.
    pub container_unix: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ScanRequest {
    pub input: PathBuf,
    pub bundles_dir: PathBuf,
    pub bundle_id: String,
    pub posters_dir: PathBuf,
    pub out_dir: PathBuf,
    pub detection: DetectionSize,
    pub coverage: Coverage,
    pub form_factor: FormFactor,
    pub execution: Execution,
    pub missing: bool,
    pub wanted: bool,
    pub abort_after_frames: Option<u64>,
    /// Stills already decoded by the platform. Desktop file scans leave this empty.
    pub frames_dir: Option<PathBuf>,
    /// When set, estimate and the frame rate come from the platform decoder.
    pub media: Option<MediaFacts>,
    pub now: SystemTime,
}

#[derive(Clone, Debug)]
pub struct ScanOpts {
    pub detection: DetectionSize,
    pub coverage: Coverage,
    pub execution: Execution,
    pub form_factor: FormFactor,
    pub missing: bool,
    pub wanted: bool,
    pub abort_after_frames: Option<u64>,
    pub fps: f64,
    pub frame_count_hint: Option<u64>,
    pub warnings: Vec<String>,
    pub out_dir: Option<PathBuf>,
}

pub fn media_warnings(mtime: Option<SystemTime>, container: Option<SystemTime>, now: SystemTime) -> Vec<String> {
    let mut warnings = Vec::new();
    let Some(mtime) = mtime else {
        return warnings;
    };
    if let Ok(age) = now.duration_since(mtime) {
        if age > Duration::from_secs(30 * 24 * 3600) {
            warnings.push(crate::copy::OLD_FILE.to_string());
        }
    }
    if let Some(container) = container {
        let delta = if let Ok(d) = mtime.duration_since(container) {
            d
        } else {
            container.duration_since(mtime).unwrap_or_default()
        };
        if delta > Duration::from_secs(24 * 3600) {
            warnings.push(crate::copy::TIMESTAMP_DISAGREE.to_string());
        }
    }
    warnings
}


fn open_models(bundle: &Bundle, pack: &PosterPack) -> Result<Option<FaceModels>, ScanReport> {
    if pack.perception != PERCEPTION_ONNX {
        return Ok(None);
    }
    if !bundle.weights_ready {
        return Err(refused(
            "missing_weights",
            "The bundle weights are not installed, or their SHA-256 is not pinned. Refusing.",
        ));
    }
    crate::onnx_exec::load(bundle).map(Some).map_err(|message| refused("missing_weights", &message))
}

pub fn scan_path(req: &ScanRequest, progress: &mut dyn FnMut(Progress)) -> ScanReport {
    let bundle = match crate::bundle::load_bundles(&req.bundles_dir)
        .ok()
        .and_then(|all| all.into_iter().find(|b| b.id == req.bundle_id))
    {
        Some(bundle) => bundle,
        None => return refused("bundle_not_found", "That bundle is not in the catalog."),
    };
    let pack = match crate::posters::load_pack(&req.posters_dir, req.now) {
        Ok(pack) => pack,
        Err(err) => {
            let code = match err {
                PackError::Missing => "missing_pack",
                PackError::BadHash | PackError::Unreadable => "bad_hash",
                PackError::Expired => "expired_pack",
            };
            return refused(code, err.refusal());
        }
    };
    if pack.perception == PERCEPTION_ONNX && !bundle.weights_ready {
        return refused(
            "missing_weights",
            "The bundle weights are not installed, or their SHA-256 is not pinned. Refusing.",
        );
    }
    if pack.perception != PERCEPTION_FIDUCIAL && pack.perception != PERCEPTION_ONNX {
        return refused("bad_hash", "The poster pack perception is not recognized. Refusing.");
    }
    if req.frames_dir.is_some() {
        return scan_decoded_dir(req, &bundle, &pack, progress);
    }
    let probed = match decode::probe(&req.input) {
        Ok(probe) => probe,
        Err(MediaError::BadCodec) => {
            return refused("bad_codec", "Bad codec or unreadable file. Refusing.")
        }
    };
    let mtime = fs::metadata(&req.input).and_then(|m| m.modified()).ok();
    let warnings = media_warnings(mtime, probed.container_created, req.now);
    if let Err(report) = prepare_out(&req.out_dir) {
        return report;
    }
    let opts = ScanOpts {
        detection: req.detection,
        coverage: req.coverage,
        execution: req.execution,
        form_factor: req.form_factor,
        missing: req.missing,
        wanted: req.wanted,
        abort_after_frames: req.abort_after_frames,
        fps: probed.fps,
        frame_count_hint: if probed.video { Some(probed.frames) } else { Some(1) },
        warnings,
        out_dir: Some(req.out_dir.clone()),
    };
        let models = match open_models(&bundle, &pack) {
        Ok(models) => models,
        Err(report) => return report,
    };
    let mut engine = Engine::new(&bundle, &pack, &opts, models);
    let decoded = decode::for_each_frame(&req.input, |index, frame| {
        let keep = engine.push(index, frame, progress);
        keep
    });
    let stats = match decoded {
        Ok(stats) => stats,
        Err(MediaError::BadCodec) => {
            return refused("bad_codec", "Bad codec or unreadable file. Refusing.")
        }
    };
    engine.frames_decoded = stats.frames_decoded;
    if !stats.clean {
        engine.stopped = true;
        if engine.incomplete_reason.is_none() {
            engine.incomplete_reason = Some("The file was not fully decoded.".into());
        }
    }
    // Only an exact frame count can say a clean decode stopped early. Duration
    // times the frame rate also counts audio that continues after the pictures,
    // and that estimate is often higher than the frames the decoder finished.
    if req.coverage == Coverage::Complete
        && probed.video
        && probed.frames_exact
        && probed.frames > 0
        && stats.frames_decoded < probed.frames
    {
        engine.stopped = true;
        engine.incomplete_reason = Some("The file was not fully decoded.".into());
    }
    let mut report = engine.finish();
    if let Err(err) = write_report(&req.out_dir, &report) {
        report.status = "incomplete".into();
        report.summary = INCOMPLETE.into();
        report.message = err;
    }
    report
}

fn scan_decoded_dir(
    req: &ScanRequest,
    bundle: &Bundle,
    pack: &PosterPack,
    progress: &mut dyn FnMut(Progress),
) -> ScanReport {
    let dir = match &req.frames_dir {
        Some(dir) => dir.as_path(),
        None => return refused("bad_codec", "Bad codec or unreadable file. Refusing."),
    };
    let frames = match decode::load_frame_dir(dir) {
        Ok(frames) => frames,
        Err(MediaError::BadCodec) => {
            return refused("bad_codec", "Bad codec or unreadable file. Refusing.")
        }
    };
    let fps = req.media.as_ref().map(|media| media.fps).unwrap_or(0.0);
    if frames.len() > 1 && fps <= 0.0 {
        return refused("bad_codec", "The decoder did not report a frame rate. Refusing.");
    }
    let mtime = fs::metadata(&req.input).and_then(|meta| meta.modified()).ok();
    let container = req
        .media
        .as_ref()
        .and_then(|media| media.container_unix)
        .map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
        .or_else(|| decode::container_created(&req.input));
    let warnings = media_warnings(mtime, container, req.now);
    if let Err(report) = prepare_out(&req.out_dir) {
        return report;
    }
    let opts = ScanOpts {
        detection: req.detection,
        coverage: req.coverage,
        execution: req.execution,
        form_factor: req.form_factor,
        missing: req.missing,
        wanted: req.wanted,
        abort_after_frames: req.abort_after_frames,
        fps,
        frame_count_hint: Some(frames.len() as u64),
        warnings,
        out_dir: Some(req.out_dir.clone()),
    };
        let models = match open_models(bundle, pack) {
        Ok(models) => models,
        Err(report) => return report,
    };
    let mut engine = Engine::new(bundle, pack, &opts, models);
    for (index, frame) in frames.iter().enumerate() {
        if !engine.push(index as u64, frame, progress) {
            break;
        }
    }
    engine.frames_decoded = frames.len() as u64;
    let mut report = engine.finish();
    if let Err(err) = write_report(&req.out_dir, &report) {
        report.status = "incomplete".into();
        report.summary = INCOMPLETE.into();
        report.message = err;
    }
    report
}

pub fn scan_images(
    frames: &[RgbImage],
    bundle: &Bundle,
    pack: &PosterPack,
    opts: &ScanOpts,
    progress: &mut dyn FnMut(Progress),
) -> ScanReport {
    if pack.perception == PERCEPTION_ONNX && !bundle.weights_ready {
        return refused(
            "missing_weights",
            "The bundle weights are not installed, or their SHA-256 is not pinned. Refusing.",
        );
    }
    if let Some(dir) = &opts.out_dir {
        if let Err(report) = prepare_out(dir) {
            return report;
        }
    }
        let models = match open_models(bundle, pack) {
        Ok(models) => models,
        Err(report) => return report,
    };
    let mut engine = Engine::new(bundle, pack, opts, models);
    for (ordinal, frame) in frames.iter().enumerate() {
        if !engine.push(ordinal as u64, frame, progress) {
            break;
        }
    }
    engine.frames_decoded = frames.len() as u64;
    let report = engine.finish();
    if let Some(dir) = &opts.out_dir {
        let _ = write_report(dir, &report);
    }
    report
}

struct Engine<'a> {
    bundle: &'a Bundle,
    pack: &'a PosterPack,
    opts: &'a ScanOpts,
    tracker: ByteTrack,
    models: Option<FaceModels>,
    runtime_failure: Option<String>,
    next_sample: f64,
    inventory: Vec<InventoryItem>,
    candidates: Vec<Candidate>,
    comparisons: Vec<Comparison>,
    faces_embedded: u64,
    plates_ocr_attempted: u64,
    faces_seen_not_compared: u64,
    frames_analyzed: u64,
    frames_decoded: u64,
    stopped: bool,
    incomplete_reason: Option<String>,
}

impl<'a> Engine<'a> {
    fn new(bundle: &'a Bundle, pack: &'a PosterPack, opts: &'a ScanOpts, models: Option<FaceModels>) -> Self {
        Self {
            bundle,
            pack,
            opts,
            tracker: ByteTrack::new(),
            models,
            runtime_failure: None,
            next_sample: 0.0,
            inventory: Vec::new(),
            candidates: Vec::new(),
            comparisons: Vec::new(),
            faces_embedded: 0,
            plates_ocr_attempted: 0,
            faces_seen_not_compared: 0,
            frames_analyzed: 0,
            frames_decoded: 0,
            stopped: false,
            incomplete_reason: None,
        }
    }

    fn push(&mut self, index: u64, frame: &RgbImage, progress: &mut dyn FnMut(Progress)) -> bool {
        if self.stopped {
            return false;
        }
        if let Some(limit) = self.opts.abort_after_frames {
            if self.frames_analyzed >= limit {
                self.stopped = true;
                self.incomplete_reason = Some("The scan stopped before every selected frame was analyzed.".into());
                return false;
            }
        }
        if !self.take_sample(index) {
            return true;
        }
        let (det_img, map) = detection_image(frame, self.opts.detection);
        let hits = if self.models.is_some() {
            match self.models.as_mut().unwrap().detect(&det_img) {
                Ok(hits) => hits,
                Err(message) => {
                    self.runtime_failure = Some(message);
                    self.stopped = true;
                    return false;
                }
            }
        } else if self.pack.perception == PERCEPTION_FIDUCIAL {
            fiducial::detect(&det_img)
        } else {
            self.runtime_failure = Some("The detector did not load. Refusing.".into());
            self.stopped = true;
            return false;
        };
        let mut track_dets = Vec::new();
        for hit in &hits {
            let class = match hit.marker {
                Marker::Face { .. } => {
                    if hit.rect.short() < FACE_SEEN_PX {
                        continue;
                    }
                    TrackClass::Face
                }
                Marker::Vehicle => TrackClass::Vehicle,
                Marker::Plate => TrackClass::Plate,
            };
            track_dets.push(TrackDet { class, rect: hit.rect, score: hit.score });
        }
        let ids = self.tracker.update(&track_dets);
        let mut id_at = 0usize;
        for hit in &hits {
            let seen_face = matches!(hit.marker, Marker::Face { .. }) && hit.rect.short() >= FACE_SEEN_PX;
            let trackable = seen_face || !matches!(hit.marker, Marker::Face { .. });
            if !trackable {
                continue;
            }
            let track_id = ids.get(id_at).copied().unwrap_or(0);
            id_at += 1;
            self.ingest(index, frame, &det_img, map, hit, track_id, progress);
        }
        self.frames_analyzed += 1;
        true
    }

    /// `measured` keeps about 5 frames a second. `complete` keeps every frame.
    fn take_sample(&mut self, index: u64) -> bool {
        if self.opts.coverage != Coverage::Measured {
            return true;
        }
        let step = if self.opts.fps <= 5.0 { 1.0 } else { self.opts.fps / 5.0 };
        if (index as f64) + 1e-6 < self.next_sample {
            return false;
        }
        if index < self.next_sample.floor() as u64 {
            return false;
        }
        self.next_sample += step;
        true
    }

    fn ingest(
        &mut self,
        index: u64,
        frame: &RgbImage,
        _det_img: &RgbImage,
        map: FrameMap,
        hit: &fiducial::Hit,
        track_id: u64,
        progress: &mut dyn FnMut(Progress),
    ) {
        let orig = map.to_original(hit.rect);
        let Some(orig) = orig.clamp(frame.width(), frame.height()) else {
            return;
        };
        match hit.marker {
            Marker::Face { .. } => {
                let local = hit.landmarks.map(|points| landmarks_in_crop(map, points, orig));
                self.ingest_face(index, frame, hit.rect, orig, track_id, local, progress);
            }
            Marker::Vehicle => {
                let crop = self.write_crop(index, frame, orig, "vehicle");
                let item = InventoryItem {
                    kind: "vehicle".into(),
                    frame_index: index,
                    frame_label: frame_label(index),
                    track_id,
                    det_short_px: hit.rect.short(),
                    orig_short_px: orig.short(),
                    label: VEHICLE_NOT_PERSON.into(),
                    compared: false,
                    crop: crop.clone(),
                    fiducial_id: None,
                };
                progress(Progress { frame_index: index, frame_label: frame_label(index), label: item.label.clone(), kind: "vehicle".into(), crop: crop.clone() });
                self.inventory.push(item);
            }
            Marker::Plate => self.ingest_plate(index, frame, hit.rect, orig, track_id, progress),
        }
    }

    fn ingest_face(
        &mut self,
        index: u64,
        frame: &RgbImage,
        det_rect: Rect,
        orig: Rect,
        track_id: u64,
        landmarks: Option<[(f32, f32); 5]>,
        progress: &mut dyn FnMut(Progress),
    ) {
        let crop_img = geom::crop(frame, orig);
        let crop_path = self.write_crop(index, frame, orig, "face");
        if orig.short() < FACE_COMPARE_PX {
            self.faces_seen_not_compared += 1;
            let item = InventoryItem {
                kind: "face".into(),
                frame_index: index,
                frame_label: frame_label(index),
                track_id,
                det_short_px: det_rect.short(),
                orig_short_px: orig.short(),
                label: NOT_COMPARED.into(),
                compared: false,
                crop: crop_path.clone(),
                fiducial_id: None,
            };
            progress(Progress { frame_index: index, frame_label: frame_label(index), label: NOT_COMPARED.into(), kind: "face".into(), crop: crop_path });
            self.inventory.push(item);
            return;
        }
        let Some(crop_img) = crop_img else {
            return;
        };
        if self.models.is_some() {
            let embedded = self.models.as_mut().unwrap().embed(&crop_img, landmarks);
            let embedding = match embedded {
                Ok(embedding) => embedding,
                Err(message) => {
                    self.runtime_failure = Some(message);
                    self.stopped = true;
                    return;
                }
            };
            self.faces_embedded += 1;
            self.score_face(index, frame, det_rect, orig, track_id, &embedding, 0, crop_path, progress);
            return;
        }
        let Some(id) = fiducial::decode_face(&crop_img) else {
            let item = InventoryItem {
                kind: "face".into(),
                frame_index: index,
                frame_label: frame_label(index),
                track_id,
                det_short_px: det_rect.short(),
                orig_short_px: orig.short(),
                label: FACE_UNSCORED.into(),
                compared: false,
                crop: crop_path.clone(),
                fiducial_id: None,
            };
            progress(Progress { frame_index: index, frame_label: frame_label(index), label: item.label.clone(), kind: "face".into(), crop: crop_path });
            self.inventory.push(item);
            return;
        };
        self.faces_embedded += 1;
        let (probe, _self_score) = fixture_probe(id, orig.x, orig.y, orig.w, orig.h, index);
        let enabled = self.pack.posters.iter().filter(|p| class_on(p, self.opts)).collect::<Vec<_>>();
        let mut best: Option<(&Poster, f32)> = None;
        for poster in &enabled {
            let Some(embedding) = poster.embedding.as_ref() else {
                continue;
            };
            let score = crate::embed::cosine(&probe, embedding);
            let passed = score >= self.bundle.threshold;
            self.comparisons.push(Comparison {
                frame_index: index,
                poster_id: poster.id.clone(),
                cosine: score,
                passed,
                fiducial_id: id,
                poster_fiducial_id: poster.fiducial_id,
            });
            if best.map(|(_, s)| score > s).unwrap_or(true) {
                best = Some((poster, score));
            }
            if passed {
                let frame_path = self.write_frame(index, frame);
                self.candidates.push(Candidate {
                    wording: POSSIBLE_CANDIDATE.into(),
                    kind: "face".into(),
                    frame_index: index,
                    frame_label: frame_label(index),
                    track_id,
                    cosine: Some(score),
                    threshold: self.bundle.threshold,
                    uncertainty: face_uncertainty(score, self.bundle.threshold, &self.bundle.name),
                    poster_id: poster.id.clone(),
                    poster_class: poster.class.as_str().into(),
                    poster_class_label: poster.class.label().into(),
                    poster_title: poster.title.clone(),
                    fbi_url: poster.fbi_url.clone(),
                    plate: None,
                    crop: crop_path.clone(),
                    frame: frame_path,
                    leaving: crate::copy::LEAVING.into(),
                });
            }
        }
        let label = if self.candidates.iter().any(|c| c.frame_index == index && c.kind == "face" && c.track_id == track_id)
        {
            POSSIBLE_CANDIDATE
        } else {
            BELOW_CUTOFF
        };
        let item = InventoryItem {
            kind: "face".into(),
            frame_index: index,
            frame_label: frame_label(index),
            track_id,
            det_short_px: det_rect.short(),
            orig_short_px: orig.short(),
            label: label.into(),
            compared: true,
            crop: crop_path.clone(),
            fiducial_id: Some(id),
        };
        progress(Progress { frame_index: index, frame_label: frame_label(index), label: label.into(), kind: "face".into(), crop: crop_path });
        self.inventory.push(item);
        let _ = best;
    }

    fn score_face(
        &mut self,
        index: u64,
        frame: &RgbImage,
        det_rect: Rect,
        orig: Rect,
        track_id: u64,
        probe: &[f32],
        id: u16,
        crop_path: Option<String>,
        progress: &mut dyn FnMut(Progress),
    ) {
        let enabled = self.pack.posters.iter().filter(|p| class_on(p, self.opts)).collect::<Vec<_>>();
        let mut best: Option<(&Poster, f32)> = None;
        for poster in &enabled {
            let Some(embedding) = poster.embedding.as_ref() else {
                continue;
            };
            let score = crate::embed::cosine(&probe, embedding);
            let passed = score >= self.bundle.threshold;
            self.comparisons.push(Comparison {
                frame_index: index,
                poster_id: poster.id.clone(),
                cosine: score,
                passed,
                fiducial_id: id,
                poster_fiducial_id: poster.fiducial_id,
            });
            if best.map(|(_, s)| score > s).unwrap_or(true) {
                best = Some((poster, score));
            }
            if passed {
                let frame_path = self.write_frame(index, frame);
                self.candidates.push(Candidate {
                    wording: POSSIBLE_CANDIDATE.into(),
                    kind: "face".into(),
                    frame_index: index,
                    frame_label: frame_label(index),
                    track_id,
                    cosine: Some(score),
                    threshold: self.bundle.threshold,
                    uncertainty: face_uncertainty(score, self.bundle.threshold, &self.bundle.name),
                    poster_id: poster.id.clone(),
                    poster_class: poster.class.as_str().into(),
                    poster_class_label: poster.class.label().into(),
                    poster_title: poster.title.clone(),
                    fbi_url: poster.fbi_url.clone(),
                    plate: None,
                    crop: crop_path.clone(),
                    frame: frame_path,
                    leaving: crate::copy::LEAVING.into(),
                });
            }
        }
        let label = if self.candidates.iter().any(|c| c.frame_index == index && c.kind == "face" && c.track_id == track_id)
        {
            POSSIBLE_CANDIDATE
        } else {
            BELOW_CUTOFF
        };
        let item = InventoryItem {
            kind: "face".into(),
            frame_index: index,
            frame_label: frame_label(index),
            track_id,
            det_short_px: det_rect.short(),
            orig_short_px: orig.short(),
            label: label.into(),
            compared: true,
            crop: crop_path.clone(),
            fiducial_id: Some(id),
        };
        progress(Progress { frame_index: index, frame_label: frame_label(index), label: label.into(), kind: "face".into(), crop: crop_path });
        self.inventory.push(item);
        let _ = best;
    }


    fn ingest_plate(
        &mut self,
        index: u64,
        frame: &RgbImage,
        det_rect: Rect,
        orig: Rect,
        track_id: u64,
        progress: &mut dyn FnMut(Progress),
    ) {
        let crop_path = self.write_crop(index, frame, orig, "plate");
        let quality = orig.w >= PLATE_MIN_WIDTH && orig.h >= PLATE_MIN_HEIGHT;
        let posters = self.pack.posters.iter().filter(|p| class_on(p, self.opts)).collect::<Vec<_>>();
        let published = posters.iter().any(|p| p.plate.as_ref().is_some_and(|s| !s.is_empty()));
        if !quality || !published {
            let item = InventoryItem {
                kind: "plate".into(),
                frame_index: index,
                frame_label: frame_label(index),
                track_id,
                det_short_px: det_rect.short(),
                orig_short_px: orig.short(),
                label: if !quality { NOT_COMPARED } else { PLATE_UNPUBLISHED }.into(),
                compared: false,
                crop: crop_path.clone(),
                fiducial_id: None,
            };
            progress(Progress { frame_index: index, frame_label: frame_label(index), label: item.label.clone(), kind: "plate".into(), crop: crop_path });
            self.inventory.push(item);
            return;
        }
        self.plates_ocr_attempted += 1;
        let text = geom::crop(frame, orig).as_ref().and_then(fiducial::ocr_plate);
        let matched = text.as_ref().and_then(|text| {
            let norm = normalize_plate(text);
            posters.into_iter().find(|p| p.plate.as_ref().is_some_and(|plate| normalize_plate(plate) == norm))
        });
        let read = text.is_some();
        if let (Some(poster), Some(text)) = (matched, text) {
            let frame_path = self.write_frame(index, frame);
            self.candidates.push(Candidate {
                wording: POSSIBLE_CANDIDATE.into(),
                kind: "plate".into(),
                frame_index: index,
                frame_label: frame_label(index),
                track_id,
                cosine: None,
                threshold: self.bundle.threshold,
                uncertainty: format!("The plate reads {text}. That text is published on this poster."),
                poster_id: poster.id.clone(),
                poster_class: poster.class.as_str().into(),
                poster_class_label: poster.class.label().into(),
                poster_title: poster.title.clone(),
                fbi_url: poster.fbi_url.clone(),
                plate: Some(text),
                crop: crop_path.clone(),
                frame: frame_path,
                leaving: crate::copy::LEAVING.into(),
            });
        }
        let matched_this = self.candidates.iter().any(|c| {
            c.kind == "plate" && c.frame_index == index && c.track_id == track_id
        });
        let label = if matched_this {
            POSSIBLE_CANDIDATE
        } else if read {
            PLATE_NOT_ON_POSTER
        } else {
            PLATE_UNREAD
        };
        let item = InventoryItem {
            kind: "plate".into(),
            frame_index: index,
            frame_label: frame_label(index),
            track_id,
            det_short_px: det_rect.short(),
            orig_short_px: orig.short(),
            label: label.into(),
            compared: read,
            crop: crop_path,
            fiducial_id: None,
        };
        progress(Progress { frame_index: index, frame_label: frame_label(index), label: item.label.clone(), kind: "plate".into(), crop: item.crop.clone() });
        self.inventory.push(item);
    }

    fn write_crop(&self, index: u64, frame: &RgbImage, rect: Rect, kind: &str) -> Option<String> {
        let dir = self.opts.out_dir.as_ref()?;
        let name = format!("crops/{kind}_{index}_{}_{}.png", rect.x, rect.y);
        let path = dir.join(&name);
        fs::create_dir_all(path.parent()?).ok()?;
        let image = geom::crop(frame, rect)?;
        image.save(&path).ok()?;
        Some(name)
    }

    fn write_frame(&self, index: u64, frame: &RgbImage) -> Option<String> {
        let dir = self.opts.out_dir.as_ref()?;
        let name = format!("frames/frame_{index}.png");
        let path = dir.join(&name);
        fs::create_dir_all(path.parent()?).ok()?;
        frame.save(&path).ok()?;
        Some(name)
    }

    fn finish(self) -> ScanReport {
        if let Some(message) = self.runtime_failure {
            return refused("missing_weights", &message);
        }
        let candidates = strongest_cards(self.candidates);
        let incomplete = self.stopped || self.incomplete_reason.is_some();
        let (status, summary, message) = if incomplete {
            (
                "incomplete",
                INCOMPLETE,
                self.incomplete_reason.unwrap_or_else(|| INCOMPLETE.to_string()),
            )
        } else if candidates.is_empty() {
            ("complete", NO_CLEARANCE, NO_CLEARANCE.to_string())
        } else {
            ("complete", POSSIBLE_CANDIDATE, POSSIBLE_CANDIDATE.to_string())
        };
        let execution = self.models.as_ref().map(|models| models.execution).unwrap_or(self.opts.execution);
        let perception_note = if self.pack.perception == PERCEPTION_FIDUCIAL {
            FIXTURE_MARKERS.to_string()
        } else {
            format!("Weights from {} ran.", self.bundle.name)
        };
        ScanReport {
            status: status.into(),
            refusal: None,
            message: message.into(),
            summary: summary.into(),
            bundle_id: self.bundle.id.clone(),
            bundle_name: self.bundle.name.clone(),
            bundle_version: self.bundle.version.clone(),
            threshold: self.bundle.threshold,
            detector_declared: self.bundle.detector.clone(),
            embedder_declared: self.bundle.embedder.clone(),
            perception: self.pack.perception.clone(),
            perception_note,
            coverage: self.opts.coverage.as_str().into(),
            coverage_banner: (self.opts.coverage == Coverage::Measured).then(|| BRIEF_FACE.into()),
            frames_note: frames_note(self.frames_analyzed, self.frames_decoded),
            detection_note: Some(format!("{}.", self.opts.detection.label())),
            coverage_note: Some(self.opts.coverage.label().into()),
            class_note: Some(class_note(self.opts.missing, self.opts.wanted).into()),
            detection: self.opts.detection.as_str(),
            execution: execution.as_str().into(),
            device_note: execution.device_note().map(str::to_string),
            warnings: self.opts.warnings.clone(),
            disclosure: disclosure_lines(!incomplete),
            frames_decoded: self.frames_decoded,
            frames_analyzed: self.frames_analyzed,
            faces_embedded: self.faces_embedded,
            plates_ocr_attempted: self.plates_ocr_attempted,
            faces_seen_not_compared: self.faces_seen_not_compared,
            inventory: self.inventory,
            candidates,
            comparisons: self.comparisons,
        }
    }
}


/// One card per track and poster. The card is the frame with the highest cosine.
/// A plate has no cosine, so the earliest frame of that track is kept.
/// Inventory rows and comparison rows stay one per frame.
fn strongest_cards(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut chosen: Vec<Candidate> = Vec::new();
    for card in candidates {
        if let Some(slot) = chosen.iter_mut().find(|kept| {
            kept.kind == card.kind && kept.track_id == card.track_id && kept.poster_id == card.poster_id
        }) {
            if keeps_stronger(&card, slot) {
                *slot = card;
            }
        } else {
            chosen.push(card);
        }
    }
    chosen
}

fn face_uncertainty(score: f32, threshold: f32, bundle_name: &str) -> String {
    format!("Score {score:.2}. {bundle_name} keeps a candidate at {threshold:.2} and above.")
}

fn keeps_stronger(next: &Candidate, kept: &Candidate) -> bool {
    match (next.cosine, kept.cosine) {
        (Some(next_score), Some(kept_score)) => {
            next_score > kept_score || (next_score == kept_score && next.frame_index < kept.frame_index)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => next.frame_index < kept.frame_index,
    }
}

fn landmarks_in_crop(map: FrameMap, points: [(f32, f32); 5], orig: Rect) -> [(f32, f32); 5] {
    let scale = map.det_to_orig;
    let mut local = points;
    for point in &mut local {
        point.0 = point.0 * scale - orig.x as f32;
        point.1 = point.1 * scale - orig.y as f32;
    }
    local
}

fn class_on(poster: &Poster, opts: &ScanOpts) -> bool {
    match poster.class {
        crate::posters::PosterClass::Missing => opts.missing,
        crate::posters::PosterClass::Wanted => opts.wanted,
    }
}

fn detection_image(frame: &RgbImage, detection: DetectionSize) -> (RgbImage, FrameMap) {
    let (w, h) = frame.dimensions();
    let long = w.max(h);
    let target = match detection {
        DetectionSize::Full => long,
        DetectionSize::Px(px) => px,
    };
    geom::resize_long_side(frame, target)
}

fn prepare_out(dir: &Path) -> Result<(), ScanReport> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|_| refused("unreadable", "The output directory could not be replaced."))?;
    }
    fs::create_dir_all(dir).map_err(|_| refused("unreadable", "The output directory could not be created."))?;
    Ok(())
}

fn write_report(dir: &Path, report: &ScanReport) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?;
    fs::write(dir.join("result.json"), bytes).map_err(|_| "The result could not be written.".to_string())
}

pub fn refused(code: &str, message: &str) -> ScanReport {
    ScanReport {
        status: "refused".into(),
        refusal: Some(code.into()),
        message: message.into(),
        summary: message.into(),
        bundle_id: String::new(),
        bundle_name: String::new(),
        bundle_version: String::new(),
        threshold: 0.0,
        detector_declared: String::new(),
        embedder_declared: String::new(),
        perception: String::new(),
        perception_note: String::new(),
        coverage: String::new(),
        coverage_banner: None,
        frames_note: None,
        detection_note: None,
        coverage_note: None,
        class_note: None,
        detection: String::new(),
        execution: String::new(),
        device_note: None,
        warnings: Vec::new(),
        disclosure: disclosure_lines(false),
        frames_decoded: 0,
        frames_analyzed: 0,
        faces_embedded: 0,
        plates_ocr_attempted: 0,
        faces_seen_not_compared: 0,
        inventory: Vec::new(),
        candidates: Vec::new(),
        comparisons: Vec::new(),
    }
}

/// A finished scan keeps the clearance sentence. An unfinished or refused scan does not.
fn disclosure_lines(include_clearance: bool) -> Vec<String> {
    DISCLOSURE
        .iter()
        .copied()
        .filter(|line| include_clearance || *line != NO_CLEARANCE)
        .map(str::to_string)
        .collect()
}

pub fn delete_output(dir: &Path) -> Result<(), String> {
    if !dir.join("result.json").is_file() {
        return Err("Refusing to delete a directory that is not an OpenWorld result.".into());
    }
    fs::remove_dir_all(dir).map_err(|_| "The result could not be deleted.".to_string())
}

#[derive(Clone, Debug, Serialize)]
pub struct LeavePrompt {
    pub message: String,
    pub url: String,
}

pub fn leave_prompt(url: &str) -> Result<LeavePrompt, String> {
    let ok = url == "https://www.fbi.gov"
        || url.starts_with("https://www.fbi.gov/")
        || url == "https://fbi.gov"
        || url.starts_with("https://fbi.gov/");
    if !ok {
        return Err("OpenWorld only opens an FBI page.".into());
    }
    Ok(LeavePrompt { message: crate::copy::LEAVING.into(), url: url.into() })
}

pub fn estimate_for(req: &ScanRequest) -> Result<crate::estimate::Estimate, ScanReport> {
    let bundle = crate::bundle::load_bundles(&req.bundles_dir)
        .ok()
        .and_then(|all| all.into_iter().find(|b| b.id == req.bundle_id))
        .ok_or_else(|| refused("bundle_not_found", "That bundle is not in the catalog."))?;
    let (frames, fps, duration, long_side) = if let Some(media) = &req.media {
        if media.width == 0 || media.height == 0 || media.frames == 0 {
            return Err(refused("bad_codec", "Bad codec or unreadable file. Refusing."));
        }
        (media.frames, media.fps, media_duration(media), media.width.max(media.height))
    } else {
        let probed = decode::probe(&req.input).map_err(|_| refused("bad_codec", "Bad codec or unreadable file. Refusing."))?;
        let duration = if probed.video {
            if probed.duration_sec > 0.0 {
                probed.duration_sec
            } else if probed.fps > 0.0 {
                probed.frames as f64 / probed.fps
            } else {
                0.0
            }
        } else {
            0.0
        };
        (probed.frames.max(1), probed.fps, duration, probed.width.max(probed.height))
    };
    Ok(crate::estimate::estimate(&crate::estimate::EstimateInput {
        frames: frames.max(1),
        fps,
        duration_sec: duration,
        original_long_side: long_side,
        detection: req.detection,
        coverage: req.coverage,
        bundle_factor: bundle.estimate_factor,
        execution: req.execution,
        form_factor: req.form_factor,
    }))
}

fn media_duration(media: &MediaFacts) -> f64 {
    if !media.video {
        return 0.0;
    }
    if media.duration_sec > 0.0 {
        media.duration_sec
    } else if media.fps > 0.0 {
        media.frames as f64 / media.fps
    } else {
        0.0
    }
}

#[cfg(test)]
#[path = "scan_critical.rs"]
mod critical_tests;
