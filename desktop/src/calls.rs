// SPDX-License-Identifier: Apache-2.0

//! Typed calls into the scan library, in this process.
//!
//! Each goes through `invoke::run`, the same contract the phones use, and the
//! JSON answer is read back into Rust types. These block, so the window runs
//! them on a worker thread ([`run`]); tests call them directly.

use crate::places::{self, Places};
use openworld_core::copy;
use openworld_core::decode;
use openworld_core::estimate::Estimate;
use openworld_core::invoke::{self, Call};
use openworld_core::report::{self, PackInfo, ScanReport};
use openworld_core::{media_warnings, Event, Line, ScanRequest, StopFlag};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Thumbnails are this many pixels square: the middle of the first frame.
const THUMB_SIDE: u32 = 256;

/// One row of the bundle picker.
#[derive(Clone, Debug, Deserialize)]
pub struct BundleRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub best_for: String,
    pub preselected: bool,
    pub curve_exists: bool,
    pub curve_line: Line,
    pub weights_ready: bool,
    pub official: bool,
    #[serde(default)]
    pub unofficial_notice: Option<String>,
}

/// What the file card shows. Read with the core's own decoder.
#[derive(Clone, Debug)]
pub struct FileFacts {
    pub video: bool,
    pub width: u32,
    pub height: u32,
    pub duration_sec: f64,
    pub bytes: u64,
    /// The container's creation time, else the file's modified time.
    pub recorded_unix: Option<i64>,
    /// Old file, timestamps disagree.
    pub warnings: Vec<Line>,
    /// The middle square of the first frame, upright, as RGB.
    pub thumb: Option<Thumb>,
}

#[derive(Clone, Debug)]
pub struct Thumb {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// Work for the worker thread.
pub enum Job {
    /// Sweep expired results and old temporary copies, then read bundles and posters.
    Startup,
    Posters,
    InstallDemo,
    Facts {
        ticket: u64,
        path: PathBuf,
        mtime_unix: Option<i64>,
    },
    Estimate {
        ticket: u64,
        req: Box<ScanRequest>,
    },
    Scan {
        req: Box<ScanRequest>,
        stop: StopFlag,
    },
    Delete {
        out: PathBuf,
    },
}

/// What comes back to the window.
pub enum Msg {
    Bundles(Result<Vec<BundleRow>, Line>),
    Posters(Result<PackInfo, Line>),
    Facts { ticket: u64, result: Result<FileFacts, Line> },
    Estimate { ticket: u64, result: Result<Estimate, Line> },
    Event(Event),
    ScanDone(Box<ScanReport>),
    Deleted(Result<Line, Line>),
}

/// Run one job to its end. `emit` may be called many times (scan events).
/// A panic in the library still answers, so the window never waits forever.
pub fn run(job: Job, places: &Places, emit: &mut dyn FnMut(Msg)) {
    let failed = failure(&job);
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_job(job, places, emit)));
    if ran.is_err() {
        emit(failed());
    }
}

/// The answer for a job the library panicked in. A scan's partial folder is removed.
fn failure(job: &Job) -> Box<dyn FnOnce() -> Msg> {
    fn internal<T>() -> Result<T, Line> {
        Err(Line::of(copy::R_INTERNAL))
    }
    match job {
        Job::Startup => Box::new(move || Msg::Bundles(internal())),
        Job::Posters | Job::InstallDemo => Box::new(move || Msg::Posters(internal())),
        Job::Facts { ticket, .. } => {
            let ticket = *ticket;
            Box::new(move || Msg::Facts { ticket, result: internal() })
        }
        Job::Estimate { ticket, .. } => {
            let ticket = *ticket;
            Box::new(move || Msg::Estimate { ticket, result: internal() })
        }
        Job::Scan { req, .. } => {
            let out = req.out.clone();
            Box::new(move || {
                let _ = delete(&out);
                Msg::ScanDone(Box::new(report::refused(copy::R_INTERNAL)))
            })
        }
        Job::Delete { .. } => Box::new(move || Msg::Deleted(internal())),
    }
}

fn run_job(job: Job, places: &Places, emit: &mut dyn FnMut(Msg)) {
    match job {
        Job::Startup => {
            sweep(&places.results);
            places::sweep_imports(&std::env::temp_dir(), SystemTime::now());
            emit(Msg::Bundles(bundles(&places.bundles)));
            emit(Msg::Posters(posters_check(&places.posters)));
        }
        Job::Posters => emit(Msg::Posters(posters_check(&places.posters))),
        Job::InstallDemo => emit(Msg::Posters(posters_demo(&places.posters))),
        Job::Facts { ticket, path, mtime_unix } => emit(Msg::Facts { ticket, result: file_facts(&path, mtime_unix) }),
        Job::Estimate { ticket, req } => emit(Msg::Estimate { ticket, result: estimate(&req) }),
        Job::Scan { req, stop } => {
            let report = scan(&req, &stop, &mut |event| emit(Msg::Event(event)));
            emit(Msg::ScanDone(Box::new(report)));
        }
        Job::Delete { out } => emit(Msg::Deleted(delete(&out))),
    }
}

fn refusal_line(value: &Value) -> Line {
    serde_json::from_value(value["headline"].clone()).unwrap_or_else(|_| Line::of(copy::R_BAD_REQUEST))
}

/// An answer, or the refusal's headline.
fn answer<T: DeserializeOwned>(value: Value) -> Result<T, Line> {
    if value["ok"] != Value::Bool(true) {
        return Err(refusal_line(&value));
    }
    serde_json::from_value(value).map_err(|_| Line::of(copy::R_BAD_REQUEST))
}

fn call(call: Call) -> Value {
    invoke::run(call, None, &mut |_| {})
}

pub fn bundles(dir: &Path) -> Result<Vec<BundleRow>, Line> {
    #[derive(Deserialize)]
    struct Rows {
        bundles: Vec<BundleRow>,
    }
    answer::<Rows>(call(Call::Bundles { bundles: dir.into() })).map(|rows| rows.bundles)
}

pub fn estimate(req: &ScanRequest) -> Result<Estimate, Line> {
    answer(call(Call::Estimate(req.clone())))
}

/// A scan always ends in a report: complete, incomplete, or refused.
pub fn scan(req: &ScanRequest, stop: &StopFlag, events: &mut dyn FnMut(Event)) -> ScanReport {
    let value = invoke::run(Call::Scan(req.clone()), Some(stop), events);
    if value.get("schema").is_none() {
        return report::refused_line(refusal_line(&value));
    }
    // A report this shell cannot read is a version mismatch. It is never shown as a result.
    serde_json::from_value(value).unwrap_or_else(|_| report::refused(copy::R_BAD_REQUEST))
}

pub fn posters_check(dir: &Path) -> Result<PackInfo, Line> {
    #[derive(Deserialize)]
    struct Pack {
        posters: PackInfo,
    }
    answer::<Pack>(call(Call::PostersCheck { posters: dir.into() })).map(|p| p.posters)
}

/// Write the demo pack, then read it back the way a scan will.
pub fn posters_demo(dir: &Path) -> Result<PackInfo, Line> {
    let written = call(Call::PostersDemo { out: dir.into() });
    if written["ok"] != Value::Bool(true) {
        return Err(refusal_line(&written));
    }
    posters_check(dir)
}

/// The approved address and the sheet's title. Only an fbi.gov page passes.
pub fn leave(url: &str) -> Result<(Line, String), Line> {
    #[derive(Deserialize)]
    struct Leave {
        message: Line,
        url: String,
    }
    answer::<Leave>(call(Call::Leave { url: url.into() })).map(|l| (l.message, l.url))
}

pub fn delete(out: &Path) -> Result<Line, Line> {
    #[derive(Deserialize)]
    struct Deleted {
        headline: Line,
    }
    answer::<Deleted>(call(Call::Delete { out: out.into() })).map(|d| d.headline)
}

pub fn sweep(root: &Path) -> u64 {
    call(Call::Sweep { root: root.into() })["removed"].as_u64().unwrap_or(0)
}

fn unix(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

/// Size, kind, dates, warnings, and a thumbnail of the first frame.
pub fn file_facts(path: &Path, mtime_unix: Option<i64>) -> Result<FileFacts, Line> {
    let meta = std::fs::metadata(path).map_err(|_| Line::of(copy::R_UNREADABLE))?;
    let probe = decode::probe(path).map_err(|_| Line::of(copy::R_BAD_CODEC))?;
    let mtime = mtime_unix.and_then(openworld_core::timeutil::system_time_from_unix).or_else(|| meta.modified().ok());
    let warnings = media_warnings(mtime, probe.container_created, SystemTime::now());
    let mut thumb = None;
    let _ = decode::for_each_frame(path, |_, frame| {
        let (w, h) = frame.dimensions();
        let side = w.min(h);
        let middle = image::imageops::crop_imm(frame, (w - side) / 2, (h - side) / 2, side, side).to_image();
        let out = THUMB_SIDE.min(side).max(1);
        let small = image::imageops::thumbnail(&middle, out, out);
        thumb = Some(Thumb { width: out, height: out, rgb: small.into_raw() });
        false
    });
    Ok(FileFacts {
        video: probe.video,
        width: probe.width,
        height: probe.height,
        duration_sec: probe.duration_sec,
        bytes: meta.len(),
        recorded_unix: probe.container_created.or(mtime).and_then(unix),
        warnings,
        thumb,
    })
}
