// SPDX-License-Identifier: Apache-2.0

//! Starting a scan: the request, the checks that refuse it, and the desktop
//! loop that decodes a file into the engine. A phone uses the same `open_job`
//! and pushes the frames its own decoder makes (see `ffi`).

use crate::bundle::{load_bundles, Bundle};
use crate::copy::{self, Line};
use crate::decode::{self, MediaError};
use crate::engine::{DecodeEnd, Engine, EngineConfig, Event, StopFlag};
use crate::estimate::{Coverage, Device, Estimate, EstimateInput};
use crate::outdir;
use crate::perception::{auto_long_side, Perception, Timing, PERCEPTION_FIDUCIAL};
use crate::posters::{load_pack, PosterPack};
use crate::report::{self, ScanReport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// Size and timing from AVFoundation or MediaCodec. When it is set, FFmpeg is not
/// used and the shell pushes the frames.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaFacts {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub fps: f64,
    /// Pictures the decoder will deliver. A different count pushed is incomplete.
    pub frames: u64,
    #[serde(default)]
    pub duration_sec: f64,
    #[serde(default)]
    pub video: bool,
    /// Container creation time, seconds since the Unix epoch, when the file has one.
    #[serde(default)]
    pub container_unix: Option<i64>,
}

/// One scan. Shells build it as JSON; the CLI builds it from flags.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanRequest {
    /// The file. A phone that already decoded it still names it, for its age.
    #[serde(default)]
    pub input: PathBuf,
    pub bundles: PathBuf,
    #[serde(default = "fast")]
    pub bundle: String,
    pub posters: PathBuf,
    #[serde(default)]
    pub out: PathBuf,
    #[serde(default = "complete")]
    pub coverage: Coverage,
    #[serde(default = "yes")]
    pub missing: bool,
    #[serde(default = "yes")]
    pub wanted: bool,
    /// Try the device's GPU or neural engine before the CPU.
    #[serde(default = "yes")]
    pub accelerate: bool,
    #[serde(default)]
    pub device: Device,
    #[serde(default)]
    pub media: Option<MediaFacts>,
    /// The original file's modified time, when the shell imported a copy.
    #[serde(default)]
    pub file_mtime_unix: Option<i64>,
}

fn fast() -> String {
    crate::bundle::FAST_ID.into()
}

fn complete() -> Coverage {
    Coverage::Complete
}

fn yes() -> bool {
    true
}

impl ScanRequest {
    pub fn new(input: PathBuf, bundles: PathBuf, posters: PathBuf, out: PathBuf) -> Self {
        Self {
            input,
            bundles,
            bundle: fast(),
            posters,
            out,
            coverage: Coverage::Complete,
            missing: true,
            wanted: true,
            accelerate: true,
            device: Device::default(),
            media: None,
            file_mtime_unix: None,
        }
    }
}

pub fn media_warnings(mtime: Option<SystemTime>, container: Option<SystemTime>, now: SystemTime) -> Vec<Line> {
    let mut warnings = Vec::new();
    let Some(mtime) = mtime else {
        return warnings;
    };
    if now.duration_since(mtime).is_ok_and(|age| age > Duration::from_secs(30 * 24 * 3600)) {
        warnings.push(Line::of(copy::OLD_FILE));
    }
    if let Some(container) = container {
        let delta = mtime.duration_since(container).or_else(|_| container.duration_since(mtime)).unwrap_or_default();
        if delta > Duration::from_secs(24 * 3600) {
            warnings.push(Line::of(copy::TIMESTAMP_DISAGREE));
        }
    }
    warnings
}

fn require_bundle(dir: &Path, id: &str) -> Result<Bundle, ScanReport> {
    let all = load_bundles(dir).map_err(report::refused_line)?;
    if all.is_empty() {
        return Err(report::refused(copy::R_NO_PROGRAM));
    }
    all.into_iter().find(|bundle| bundle.id == id).ok_or_else(|| report::refused(copy::R_NO_BUNDLE))
}

/// Loaded networks, kept for the next scan of the same bundle in this process.
type Cached = (String, Box<dyn Perception + Send>);
static MODELS: Mutex<Vec<Cached>> = Mutex::new(Vec::new());

/// The detector and embedder this pack is read with.
fn perception_for(bundle: &Bundle, pack: &PosterPack, accelerate: bool) -> Result<Box<dyn Perception + Send>, Line> {
    if pack.perception == PERCEPTION_FIDUCIAL {
        #[cfg(feature = "fixture")]
        return Ok(Box::new(crate::fixture::Markers));
        #[cfg(not(feature = "fixture"))]
        return Err(Line::of(copy::R_FIXTURE_OFF));
    }
    if !bundle.weights_ready {
        return Err(Line::of(copy::R_NO_WEIGHTS));
    }
    let embedder = bundle.weight("embedder").map(|w| w.sha256.as_str()).unwrap_or("");
    if pack.embedder_sha256.as_deref().map(str::trim) != Some(embedder) || embedder.is_empty() {
        return Err(Line::of(copy::R_PACK_EMBEDDER));
    }
    let key = format!("{}:{embedder}:{accelerate}", bundle.id);
    if let Ok(mut cache) = MODELS.lock() {
        if let Some(index) = cache.iter().position(|(k, _)| *k == key) {
            return Ok(cache.swap_remove(index).1);
        }
    }
    crate::onnx_exec::load(bundle, accelerate)
        .map(|models| Box::new(models) as Box<dyn Perception + Send>)
        .map_err(|message| Line::with(copy::R_RUNTIME.0, message, &[]))
}

/// Return networks for the next scan. One set per bundle is kept.
fn keep_perception(bundle: &Bundle, accelerate: bool, perception: Box<dyn Perception + Send>) {
    if perception.kind() == PERCEPTION_FIDUCIAL {
        return;
    }
    let embedder = bundle.weight("embedder").map(|w| w.sha256.clone()).unwrap_or_default();
    if let Ok(mut cache) = MODELS.lock() {
        cache.retain(|(k, _)| !k.starts_with(&format!("{}:", bundle.id)));
        cache.push((format!("{}:{embedder}:{accelerate}", bundle.id), perception));
    }
}

/// A scan ready for frames: everything that can refuse has been checked, and
/// the result folder is ours and empty.
pub struct Job {
    engine: Option<Engine>,
    bundle: Bundle,
    accelerate: bool,
    pub stop: StopFlag,
    /// Pictures the platform decoder promised, for a pushed scan.
    pub declared_frames: Option<u64>,
}

impl Job {
    pub fn engine(&mut self) -> &mut Engine {
        self.engine.as_mut().expect("job is open until finished")
    }

    pub fn finish(mut self, end: DecodeEnd, events: &mut dyn FnMut(Event)) -> ScanReport {
        let engine = self.engine.take().expect("job is finished once");
        let (report, perception) = engine.finish_keeping(end, events);
        keep_perception(&self.bundle, self.accelerate, perception);
        report
    }
}

/// Check the request and open its result folder. `warnings` are the file's age warnings.
pub fn open_job(req: &ScanRequest, fps: f64, video: bool, warnings: Vec<Line>, now: SystemTime) -> Result<Job, ScanReport> {
    let bundle = require_bundle(&req.bundles, &req.bundle)?;
    let pack = load_pack(&req.posters, now).map_err(report::refused_line)?;
    let perception = perception_for(&bundle, &pack, req.accelerate).map_err(report::refused_line)?;
    let protect: Vec<&Path> = [req.input.as_path(), req.posters.as_path(), req.bundles.as_path()].into_iter().filter(|p| !p.as_os_str().is_empty()).collect();
    outdir::prepare(&req.out, &protect).map_err(report::refused_line)?;
    let stop = StopFlag::default();
    let cfg = EngineConfig {
        coverage: req.coverage,
        missing: req.missing,
        wanted: req.wanted,
        fps,
        video,
        on_screen: None,
        cover_index: None,
        warnings,
        out_dir: Some(req.out.clone()),
        stop: stop.clone(),
        now,
    };
    let engine = Engine::new(bundle.clone(), pack, cfg, perception).map_err(report::refused_line)?;
    Ok(Job { engine: Some(engine), bundle, accelerate: req.accelerate, stop, declared_frames: None })
}

fn mtime_of(req: &ScanRequest) -> Option<SystemTime> {
    req.file_mtime_unix
        .and_then(crate::timeutil::system_time_from_unix)
        .or_else(|| fs::metadata(&req.input).and_then(|m| m.modified()).ok())
}

/// A scan of frames the platform decoder will push. Nothing is decoded here.
pub fn open_pushed_job(req: &ScanRequest, now: SystemTime) -> Result<Job, ScanReport> {
    let media = req.media.as_ref().ok_or_else(|| report::refused(copy::R_BAD_REQUEST))?;
    if media.width == 0 || media.height == 0 || media.frames == 0 {
        return Err(report::refused(copy::R_BAD_CODEC));
    }
    if u64::from(media.width) * u64::from(media.height) > decode::MAX_PIXELS {
        return Err(report::refused(copy::R_TOO_LARGE));
    }
    if !media.fps.is_finite() || (media.frames > 1 && media.fps <= 0.0) {
        return Err(report::refused(copy::R_NO_FPS));
    }
    let container = media
        .container_unix
        .and_then(crate::timeutil::system_time_from_unix)
        .or_else(|| decode::container_created(&req.input));
    let warnings = media_warnings(mtime_of(req), container, now);
    let mut job = open_job(req, media.fps, media.video || media.frames > 1, warnings, now)?;
    job.declared_frames = Some(media.frames);
    Ok(job)
}

fn media_refusal(err: MediaError) -> ScanReport {
    report::refused(match err {
        MediaError::BadCodec => copy::R_BAD_CODEC,
        MediaError::TooLarge => copy::R_TOO_LARGE,
    })
}

/// Decode a file on this computer and scan every frame the coverage keeps.
pub fn scan_path(req: &ScanRequest, stop: Option<&StopFlag>, events: &mut dyn FnMut(Event)) -> ScanReport {
    if !input_readable(&req.input) {
        return report::refused(copy::R_UNREADABLE);
    }
    let now = SystemTime::now();
    let probed = match decode::probe(&req.input) {
        Ok(probe) => probe,
        Err(err) => return media_refusal(err),
    };
    let warnings = media_warnings(mtime_of(req), probed.container_created, now);
    let mut job = match open_job(req, probed.fps, probed.video, warnings, now) {
        Ok(job) => job,
        Err(report) => return report,
    };
    if let Some(external) = stop {
        job.stop = external.clone();
    }
    {
        let stop = job.stop.clone();
        let engine = job.engine();
        engine.set_stop(stop);
        if req.coverage == Coverage::Measured && probed.video {
            engine.set_on_screen(on_screen_frames(probed.fps, probed.frames, decode::picture_delays(&req.input)));
        }
        if probed.motion_cover && probed.frames > 0 {
            engine.set_cover(probed.frames - 1);
        }
    }
    let decoded = {
        let engine = job.engine();
        decode::for_each_frame(&req.input, |index, frame| engine.push(index, frame, events))
    };
    let end = match decoded {
        // Only an exact frame count can say a clean decode stopped early. Duration
        // times the frame rate also counts audio that continues after the pictures.
        Ok(stats) => DecodeEnd {
            clean: stats.clean,
            declared_frames: (probed.video && probed.frames_exact && probed.frames > 0).then_some(stats.probe.frames),
            reason: None,
        },
        Err(err) => DecodeEnd { clean: false, declared_frames: None, reason: Some(media_refusal(err).headline) },
    };
    job.finish(end, events)
}

/// The frames a 5 Hz tick actually shows, when that set is not the even rate.
fn on_screen_frames(fps: f64, count: u64, delays: Option<Vec<f64>>) -> Option<Vec<u64>> {
    let delays = delays?;
    if count == 0 || delays.len() as u64 != count {
        return None;
    }
    let timed = crate::geom::frames_on_screen(&delays);
    if timed.is_empty() || timed == crate::geom::sample_indices(count, fps, true) {
        return None;
    }
    Some(timed)
}

/// What the scan of this file will cost, before Analyze.
pub fn estimate_for(req: &ScanRequest) -> Result<Estimate, ScanReport> {
    let now = SystemTime::now();
    let (frames, fps, duration, long, short, cover, delays) = if let Some(media) = &req.media {
        if media.width == 0 || media.height == 0 || media.frames == 0 {
            return Err(report::refused(copy::R_BAD_CODEC));
        }
        if !media.fps.is_finite() || (media.frames > 1 && media.fps <= 0.0) {
            return Err(report::refused(copy::R_NO_FPS));
        }
        let duration = if media.duration_sec > 0.0 { media.duration_sec } else if media.fps > 0.0 { media.frames as f64 / media.fps } else { 0.0 };
        (media.frames, media.fps, duration, media.width.max(media.height), media.width.min(media.height), false, None)
    } else {
        if !input_readable(&req.input) {
            return Err(report::refused(copy::R_UNREADABLE));
        }
        let probed = decode::probe(&req.input).map_err(media_refusal)?;
        let duration = if !probed.video {
            0.0
        } else if probed.duration_sec > 0.0 {
            probed.duration_sec
        } else if probed.fps > 0.0 {
            probed.frames as f64 / probed.fps
        } else {
            0.0
        };
        let delays = (req.coverage == Coverage::Measured && probed.video).then(|| decode::picture_delays(&req.input)).flatten();
        (probed.frames.max(1), probed.fps, duration, probed.width.max(probed.height), probed.width.min(probed.height), probed.motion_cover, delays)
    };
    let bundle = require_bundle(&req.bundles, &req.bundle)?;
    let pack = load_pack(&req.posters, now).ok();
    let (min_face, timing, provider) = timing_for(&bundle, pack.as_ref(), req.accelerate, long);
    let det_long = auto_long_side(long, min_face);
    let (mut frames, mut fps, mut cover) = (frames, fps, cover);
    if req.coverage == Coverage::Measured {
        if let Some(keep) = on_screen_frames(fps, frames, delays) {
            frames = keep.len() as u64;
            fps = 1.0;
            cover = false;
        }
    }
    Ok(crate::estimate::estimate(&EstimateInput {
        frames,
        fps,
        duration_sec: duration,
        original_long_side: long,
        original_short_side: short,
        detection_long_side: det_long,
        coverage: req.coverage,
        timing,
        execution: crate::runtime::execution(&provider),
        device: req.device.clone(),
        cover,
    }))
}

/// Timings measured once per bundle, provider, and detection size in this process.
static TIMINGS: Mutex<Option<HashMap<String, (Timing, String)>>> = Mutex::new(None);

/// The smallest face the detector finds, its timing here, and the provider that ran.
fn timing_for(bundle: &Bundle, pack: Option<&PosterPack>, accelerate: bool, long: u32) -> (u32, Option<Timing>, String) {
    let demo = pack.is_some_and(|p| p.demo());
    if demo || !bundle.weights_ready {
        #[cfg(feature = "fixture")]
        if demo {
            let mut markers = crate::fixture::Markers;
            return (markers.min_face_px(), markers.time(long), "fixture".into());
        }
        return (28, None, "CPUExecutionProvider".into());
    }
    let mut perception = match crate::onnx_exec::load(bundle, accelerate) {
        Ok(models) => models,
        Err(_) => return (28, None, "CPUExecutionProvider".into()),
    };
    let det_long = auto_long_side(long, perception.min_face_px());
    let key = format!("{}:{accelerate}:{det_long}", bundle.id);
    if let Ok(cache) = TIMINGS.lock() {
        if let Some((timing, provider)) = cache.as_ref().and_then(|map| map.get(&key)) {
            return (perception.min_face_px(), Some(*timing), provider.clone());
        }
    }
    let timing = perception.time(det_long);
    let provider = perception.provider().to_string();
    if let (Some(timing), Ok(mut cache)) = (timing, TIMINGS.lock()) {
        cache.get_or_insert_with(HashMap::new).insert(key, (timing, provider.clone()));
    }
    let min = perception.min_face_px();
    keep_perception(bundle, accelerate, Box::new(perception));
    (min, timing, provider)
}

/// A regular file this process can open. A directory and a missing path are not.
pub fn input_readable(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() => fs::File::open(path).is_ok(),
        _ => false,
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LeavePrompt {
    pub message: Line,
    pub url: String,
}

/// Only the FBI's own pages open. The host is checked exactly: no look-alike, no user part.
pub fn leave_prompt(url: &str) -> Result<LeavePrompt, Line> {
    let ok = ["https://www.fbi.gov", "https://fbi.gov"]
        .iter()
        .any(|origin| url == *origin || url.strip_prefix(origin).is_some_and(|rest| rest.starts_with('/')));
    if !ok || url.chars().any(|c| c.is_whitespace() || c == '\\') {
        return Err(Line::of(copy::ONLY_FBI));
    }
    Ok(LeavePrompt { message: Line::of(copy::LEAVING), url: url.into() })
}

pub fn delete_output(dir: &Path) -> Result<Line, Line> {
    outdir::delete(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_fbi_opens() {
        for good in ["https://www.fbi.gov", "https://www.fbi.gov/wanted/topten/x", "https://fbi.gov/wanted"] {
            assert!(leave_prompt(good).is_ok(), "{good}");
        }
        for bad in [
            "http://www.fbi.gov/wanted",
            "https://www.fbi.gov.evil.example/x",
            "https://www.fbi.gov@evil.example/x",
            "https://evil.example/https://www.fbi.gov/",
            "https://www.fbi.govx/",
            "https://www.fbi.gov/ x",
            "javascript:alert(1)",
        ] {
            assert_eq!(leave_prompt(bad).unwrap_err(), Line::of(copy::ONLY_FBI), "{bad}");
        }
    }

    #[test]
    fn an_old_file_and_a_disagreeing_clock_warn() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100 * 86400);
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 86400);
        let codes: Vec<String> = media_warnings(Some(old), Some(now), now).into_iter().map(|l| l.code).collect();
        assert_eq!(codes, vec!["old_file", "timestamps_disagree"]);
        assert!(media_warnings(Some(now), Some(now), now).is_empty());
        assert!(media_warnings(None, Some(old), now).is_empty());
    }
}
