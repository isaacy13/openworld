// SPDX-License-Identifier: Apache-2.0

//! The window's state, without GTK.
//!
//! The window turns clicks into calls on [`Model`], runs the [`Job`]s it returns
//! on a worker thread, and feeds each [`Msg`] back through [`Model::apply`].
//! What a result means is read from statuses and codes, never from English.

use crate::calls::{BundleRow, FileFacts, Job, Msg};
use crate::places::Places;
use crate::strings::{self, S};
use openworld_core::copy::{self, Copy};
use openworld_core::estimate::{human_duration, Coverage, Estimate};
use openworld_core::report::{PackInfo, ScanReport, Status, TrackItem, TrackStatus};
use openworld_core::{Event, Line, ScanRequest, StopFlag, StopReason};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Debug)]
pub enum PackState {
    Checking,
    Installing,
    Ready(PackInfo),
    Refused(Line),
}

/// The file the person picked. `imported` is a temporary copy this app made.
#[derive(Clone, Debug)]
pub struct Chosen {
    pub path: PathBuf,
    pub name: String,
    pub mtime_unix: Option<i64>,
    pub imported: bool,
    /// `None` while it is read.
    pub facts: Option<Result<FileFacts, Line>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub bundle: String,
    pub coverage: Coverage,
    pub missing: bool,
    pub wanted: bool,
}

#[derive(Clone, Debug)]
pub enum EstimateState {
    Pending,
    Ready(Box<Estimate>),
    Refused(Line),
}

/// Why Analyze is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    NoFile,
    Estimating,
    Refused,
    NoClass,
}

/// One face on the scan screen. `status` is `None` until its track is decided.
#[derive(Clone, Debug)]
pub struct Face {
    pub track_id: u64,
    pub frame: u64,
    pub crop: Option<PathBuf>,
    pub status: Option<TrackStatus>,
    pub label: Option<Line>,
    pub reason: Option<Line>,
}

impl Face {
    pub fn chip(&self) -> S {
        chip_for(self.status)
    }

    pub fn tone(&self) -> Tone {
        if self.status == Some(TrackStatus::Candidate) {
            Tone::Candidate
        } else {
            Tone::Neutral
        }
    }

    /// "Face from frame 12. Possible candidate."
    pub fn accessible_label(&self) -> String {
        strings::face_label(self.frame, self.chip())
    }
}

#[derive(Clone)]
pub struct Scanning {
    pub out: PathBuf,
    pub total: u64,
    pub analyzed: u64,
    pub faces: Vec<Face>,
    pub stop: StopFlag,
    pub stopping: bool,
    pub coverage: Coverage,
    started: Instant,
}

impl std::fmt::Debug for Scanning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scanning").field("out", &self.out).field("analyzed", &self.analyzed).field("total", &self.total).finish()
    }
}

#[derive(Clone, Debug)]
pub struct Done {
    pub report: Box<ScanReport>,
    pub out: PathBuf,
    pub delete_error: Option<Line>,
}

#[derive(Clone, Debug)]
pub enum Phase {
    Start,
    Review,
    Scanning(Scanning),
    Result(Done),
}

/// The color family of a headline or chip. Each also has an icon and words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Candidate,
    Neutral,
    Incomplete,
    Error,
}

/// A line to show: from the core, or a shell label.
#[derive(Clone, Debug, PartialEq)]
pub enum Text {
    Core(Line),
    Shell(String),
}

impl Text {
    pub fn show(&self) -> String {
        match self {
            Text::Core(line) => strings::say(line).to_string(),
            Text::Shell(text) => text.clone(),
        }
    }
}

/// What Back did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Back {
    Moved,
    /// A scan is running: ask before stopping it.
    AskStop,
    Stay,
}

pub struct Model {
    pub places: Places,
    pub bundles: Option<Result<Vec<BundleRow>, Line>>,
    pub pack: PackState,
    pub file: Option<Chosen>,
    pub choice: Choice,
    pub estimate: EstimateState,
    pub phase: Phase,
    /// The window is closing: a running scan is stopped and its folder removed.
    pub closing: bool,
    /// Everything is cleaned up after `closing`.
    pub can_quit: bool,
    /// Answers for an older file or older options are dropped.
    ticket: u64,
    file_ticket: u64,
}

impl Model {
    pub fn new(places: Places) -> Self {
        Self {
            places,
            bundles: None,
            pack: PackState::Checking,
            file: None,
            choice: Choice { bundle: openworld_core::bundle::FAST_ID.into(), coverage: Coverage::Complete, missing: true, wanted: true },
            estimate: EstimateState::Pending,
            phase: Phase::Start,
            closing: false,
            can_quit: false,
            ticket: 0,
            file_ticket: 0,
        }
    }

    pub fn startup(&mut self) -> Vec<Job> {
        vec![Job::Startup]
    }

    pub fn scanning(&self) -> bool {
        matches!(self.phase, Phase::Scanning(_))
    }

    /// Offer the demo pack when there is no usable pack. Only demo posters can exist in v0.
    pub fn offers_demo(&self) -> bool {
        const OFFER: [Copy; 4] = [copy::R_PACK_MISSING, copy::R_PACK_OLD, copy::R_PACK_EXPIRED, copy::R_PACK_UNREADABLE];
        matches!(&self.pack, PackState::Refused(line) if OFFER.iter().any(|c| line.is(*c)))
    }

    pub fn install_demo(&mut self) -> Option<Job> {
        if matches!(self.pack, PackState::Installing | PackState::Ready(_)) || self.scanning() {
            return None;
        }
        self.pack = PackState::Installing;
        Some(Job::InstallDemo)
    }

    /// Poster counts for the class chips. Zero until a pack is ready.
    pub fn class_counts(&self) -> (usize, usize) {
        match &self.pack {
            PackState::Ready(info) => (info.missing, info.wanted),
            _ => (0, 0),
        }
    }

    /// Whole days since the pack was fetched.
    pub fn pack_days(&self, now: SystemTime) -> Option<u64> {
        let PackState::Ready(info) = &self.pack else {
            return None;
        };
        let created = openworld_core::timeutil::parse_rfc3339(&info.created_at)?;
        Some(now.duration_since(created).unwrap_or_default().as_secs() / 86400)
    }

    /// Open a file. A file picked while a scan runs or the window closes stays off the screen.
    pub fn choose(&mut self, path: PathBuf, name: String, mtime_unix: Option<i64>, imported: bool) -> Vec<Job> {
        if self.scanning() || self.closing {
            return Vec::new();
        }
        self.forget_file();
        self.file = Some(Chosen { path: path.clone(), name, mtime_unix, imported, facts: None });
        self.phase = Phase::Review;
        self.file_ticket += 1;
        let mut jobs = vec![Job::Facts { ticket: self.file_ticket, path, mtime_unix }];
        jobs.extend(self.estimate_job());
        jobs
    }

    fn request(&self, out: PathBuf) -> Option<ScanRequest> {
        let file = self.file.as_ref()?;
        let mut req = ScanRequest::new(file.path.clone(), self.places.bundles.clone(), self.places.posters.clone(), out);
        req.bundle = self.choice.bundle.clone();
        req.coverage = self.choice.coverage;
        req.missing = self.choice.missing;
        req.wanted = self.choice.wanted;
        req.file_mtime_unix = file.mtime_unix;
        Some(req)
    }

    fn estimate_job(&mut self) -> Option<Job> {
        let req = self.request(PathBuf::new())?;
        self.ticket += 1;
        self.estimate = EstimateState::Pending;
        Some(Job::Estimate { ticket: self.ticket, req: Box::new(req) })
    }

    pub fn set_bundle(&mut self, id: &str) -> Option<Job> {
        if self.choice.bundle == id || self.scanning() {
            return None;
        }
        self.choice.bundle = id.into();
        self.estimate_job()
    }

    pub fn set_coverage(&mut self, coverage: Coverage) -> Option<Job> {
        if self.choice.coverage == coverage || self.scanning() {
            return None;
        }
        self.choice.coverage = coverage;
        self.estimate_job()
    }

    /// Classes do not change the time, so the estimate stays.
    pub fn set_classes(&mut self, missing: bool, wanted: bool) {
        if !self.scanning() {
            self.choice.missing = missing;
            self.choice.wanted = wanted;
        }
    }

    pub fn analyze_block(&self) -> Option<Block> {
        if self.file.is_none() {
            return Some(Block::NoFile);
        }
        match &self.estimate {
            EstimateState::Pending => Some(Block::Estimating),
            EstimateState::Refused(_) => Some(Block::Refused),
            EstimateState::Ready(_) if !(self.choice.missing || self.choice.wanted) => Some(Block::NoClass),
            EstimateState::Ready(_) => None,
        }
    }

    /// Start the scan the person just saw the estimate for.
    pub fn analyze(&mut self, now: SystemTime) -> Option<Job> {
        if self.analyze_block().is_some() || !matches!(self.phase, Phase::Review) || self.closing {
            return None;
        }
        let EstimateState::Ready(estimate) = &self.estimate else {
            return None;
        };
        let total = estimate.frames_analyzed.max(1);
        let out = self.places.new_result(now);
        let req = self.request(out.clone())?;
        let stop = StopFlag::default();
        self.phase = Phase::Scanning(Scanning {
            out,
            total,
            analyzed: 0,
            faces: Vec::new(),
            stop: stop.clone(),
            stopping: false,
            coverage: self.choice.coverage,
            started: Instant::now(),
        });
        Some(Job::Scan { req: Box::new(req), stop })
    }

    /// Stop a running scan. Its report comes back Incomplete.
    pub fn stop(&mut self) {
        if let Phase::Scanning(scan) = &mut self.phase {
            scan.stop.stop(StopReason::Cancel);
            scan.stopping = true;
        }
    }

    pub fn apply(&mut self, msg: Msg) -> Vec<Job> {
        match msg {
            Msg::Bundles(result) => {
                if let Ok(rows) = &result {
                    if !rows.iter().any(|r| r.id == self.choice.bundle) {
                        if let Some(first) = rows.iter().find(|r| r.preselected).or(rows.first()) {
                            self.choice.bundle = first.id.clone();
                        }
                    }
                }
                self.bundles = Some(result);
            }
            Msg::Posters(result) => {
                self.pack = match result {
                    Ok(info) => PackState::Ready(info),
                    Err(line) => PackState::Refused(line),
                };
                // A new pack changes the estimate's timing and the class counts.
                if matches!(self.phase, Phase::Review) {
                    return self.estimate_job().into_iter().collect();
                }
            }
            Msg::Facts { ticket, result } => {
                if let Some(file) = self.file.as_mut().filter(|_| ticket == self.file_ticket) {
                    file.facts = Some(result);
                }
            }
            Msg::Estimate { ticket, result } => {
                if ticket == self.ticket {
                    self.estimate = match result {
                        Ok(estimate) => EstimateState::Ready(Box::new(estimate)),
                        Err(line) => EstimateState::Refused(line),
                    };
                }
            }
            Msg::Event(event) => self.on_event(event),
            Msg::ScanDone(report) => {
                let Phase::Scanning(scan) = &self.phase else {
                    return Vec::new();
                };
                let out = scan.out.clone();
                if self.closing {
                    self.phase = Phase::Start;
                    return vec![Job::Delete { out }];
                }
                self.phase = Phase::Result(Done { report, out, delete_error: None });
            }
            Msg::Deleted(result) => {
                if self.closing {
                    self.can_quit = true;
                } else if let Phase::Result(done) = &mut self.phase {
                    match result {
                        Ok(_) => self.reset(),
                        Err(line) => done.delete_error = Some(line),
                    }
                }
            }
        }
        Vec::new()
    }

    fn on_event(&mut self, event: Event) {
        let Phase::Scanning(scan) = &mut self.phase else {
            return;
        };
        match event {
            Event::Progress { frames_analyzed, .. } => scan.analyzed = frames_analyzed,
            Event::Face { track_id, frame, crop, .. } => {
                let crop = crop.map(|c| scan.out.join(c));
                scan.faces.push(Face { track_id, frame, crop, status: None, label: None, reason: None });
            }
            Event::Track { track } => {
                let decided = face_of(&scan.out, &track);
                match scan.faces.iter_mut().find(|f| f.track_id == track.track_id) {
                    Some(face) => *face = decided,
                    None => scan.faces.push(decided),
                }
            }
        }
    }

    /// "About 1 minute left", once enough frames are in to say.
    pub fn time_left(&self, now: Instant) -> Option<Line> {
        let Phase::Scanning(scan) = &self.phase else {
            return None;
        };
        let elapsed = now.saturating_duration_since(scan.started);
        if scan.analyzed < 3 || elapsed < Duration::from_secs(1) || scan.analyzed >= scan.total {
            return None;
        }
        let left = elapsed.as_secs_f64() / scan.analyzed as f64 * (scan.total - scan.analyzed) as f64;
        Some(human_duration(left))
    }

    pub fn delete_result(&mut self) -> Option<Job> {
        match &self.phase {
            Phase::Result(done) => Some(Job::Delete { out: done.out.clone() }),
            _ => None,
        }
    }

    pub fn back(&mut self) -> Back {
        match self.phase {
            Phase::Start => Back::Stay,
            Phase::Review => {
                self.reset();
                Back::Moved
            }
            Phase::Scanning(_) => Back::AskStop,
            Phase::Result(_) => {
                self.phase = Phase::Review;
                Back::Moved
            }
        }
    }

    /// Scan another file: back to the start, nothing chosen.
    pub fn reset(&mut self) {
        if self.scanning() {
            return;
        }
        self.forget_file();
        self.estimate = EstimateState::Pending;
        self.ticket += 1;
        self.file_ticket += 1;
        self.phase = Phase::Start;
    }

    fn forget_file(&mut self) {
        if let Some(file) = self.file.take().filter(|f| f.imported) {
            let _ = std::fs::remove_file(&file.path);
        }
    }

    /// The window is closing. True when it can go now; otherwise wait for `can_quit`.
    pub fn close(&mut self) -> bool {
        self.closing = true;
        self.forget_file();
        let _ = std::fs::remove_dir_all(&self.places.imports);
        if self.scanning() {
            self.stop();
            return false;
        }
        self.can_quit = true;
        true
    }
}

pub fn chip_for(status: Option<TrackStatus>) -> S {
    match status {
        None => S::ChipLooking,
        Some(TrackStatus::Candidate) => S::ChipCandidate,
        Some(TrackStatus::BelowCutoff) => S::ChipCompared,
        Some(TrackStatus::TooBrief) => S::ChipTooBrief,
        Some(TrackStatus::NotCompared) => S::ChipNotCompared,
        Some(TrackStatus::Unscored) => S::ChipUnscored,
    }
}

pub fn face_of(out: &std::path::Path, track: &TrackItem) -> Face {
    Face {
        track_id: track.track_id,
        frame: track.first_frame,
        crop: track.crop.as_ref().map(|c| out.join(c)),
        status: Some(track.status),
        label: Some(track.label.clone()),
        reason: track.reason.clone(),
    }
}

/// The headline card's color family, from the status.
pub fn tone(report: &ScanReport) -> Tone {
    match report.status {
        Status::Refused => Tone::Error,
        Status::Incomplete => Tone::Incomplete,
        Status::Complete if report.candidates.is_empty() => Tone::Neutral,
        Status::Complete => Tone::Candidate,
    }
}

/// The lines under the headline.
pub fn headline_detail(report: &ScanReport) -> Vec<Text> {
    let mut out = Vec::new();
    match tone(report) {
        Tone::Candidate => out.push(Text::Shell(strings::candidate_count(report.candidates.len()))),
        Tone::Neutral => out.extend(report.faces_line.clone().map(Text::Core)),
        Tone::Incomplete => {
            out.extend(report.reason.clone().map(Text::Core));
            if !report.candidates.is_empty() {
                out.push(Text::Shell(strings::candidate_count(report.candidates.len())));
            }
        }
        Tone::Error => out.push(Text::Shell(strings::text(refusal_hint(report.refusal.as_deref().unwrap_or(""))).into())),
    }
    out
}

/// What to do about a refusal, by its code.
pub fn refusal_hint(code: &str) -> S {
    let is = |c: Copy| c.0 == code;
    if [copy::R_UNREADABLE, copy::R_BAD_CODEC, copy::R_NO_FPS, copy::R_TOO_LARGE].into_iter().any(is) {
        S::HintChooseFile
    } else if is(copy::R_PACK_FUTURE) {
        S::HintCheckClock
    } else if [copy::R_NO_WEIGHTS, copy::R_RUNTIME, copy::R_PACK_EMBEDDER].into_iter().any(is) {
        S::HintInstallWeights
    } else if code.ends_with("_pack") || [copy::R_PACK_HASH, copy::R_PACK_VECTORS, copy::R_PACK_PERCEPTION].into_iter().any(is) {
        S::HintInstallDemo
    } else {
        S::HintTryAgain
    }
}

/// Faces that are not candidates, for the result's "Other faces".
pub fn other_faces(done: &Done) -> Vec<Face> {
    done.report.tracks.iter().filter(|t| t.status != TrackStatus::Candidate).map(|t| face_of(&done.out, t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls;
    use std::collections::VecDeque;

    struct Rig {
        _root: tempfile::TempDir,
        model: Model,
        input: PathBuf,
    }

    fn bundles() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bundles")
    }

    /// A model with the demo pack installed and the demo still on disk.
    fn rig() -> Rig {
        let root = tempfile::tempdir().unwrap();
        let places = Places::new(&root.path().join("data"), bundles());
        let mut model = Model::new(places);
        let jobs = model.startup();
        drive(&mut model, jobs);
        assert!(matches!(model.pack, PackState::Refused(ref l) if l.is(copy::R_PACK_MISSING)));
        assert!(model.offers_demo());
        let job = model.install_demo().unwrap();
        drive(&mut model, vec![job]);
        assert!(matches!(model.pack, PackState::Ready(ref p) if p.demo));
        let input = root.path().join("scene.png");
        openworld_core::fixture::demo_scene().image.save(&input).unwrap();
        Rig { _root: root, model, input }
    }

    /// Run jobs the way the worker does, feeding every message back.
    fn drive(model: &mut Model, jobs: Vec<Job>) {
        let mut queue: VecDeque<Job> = jobs.into();
        while let Some(job) = queue.pop_front() {
            let places = model.places.clone();
            let mut msgs = Vec::new();
            calls::run(job, &places, &mut |msg| msgs.push(msg));
            for msg in msgs {
                queue.extend(model.apply(msg));
            }
        }
    }

    fn choose(rig: &mut Rig) {
        let jobs = rig.model.choose(rig.input.clone(), "scene.png".into(), None, false);
        assert!(matches!(rig.model.phase, Phase::Review));
        assert_eq!(rig.model.analyze_block(), Some(Block::Estimating));
        drive(&mut rig.model, jobs);
    }

    fn result(model: &Model) -> &Done {
        match &model.phase {
            Phase::Result(done) => done,
            other => panic!("not a result: {other:?}"),
        }
    }

    #[test]
    fn a_demo_still_goes_from_file_to_estimate_to_a_candidate() {
        let mut rig = rig();
        assert_eq!(rig.model.class_counts(), (1, 1));
        assert_eq!(rig.model.pack_days(SystemTime::now()), Some(0));
        assert_eq!(rig.model.choice.bundle, "fast");
        choose(&mut rig);
        let facts = rig.model.file.as_ref().unwrap().facts.clone().unwrap().unwrap();
        assert!(!facts.video);
        assert_eq!((facts.width, facts.height), (640, 480));
        assert!(facts.thumb.is_some());
        let EstimateState::Ready(estimate) = &rig.model.estimate else { panic!("no estimate") };
        assert_eq!(estimate.frames_analyzed, 1);
        assert_eq!(rig.model.analyze_block(), None);

        let job = rig.model.analyze(SystemTime::now()).unwrap();
        let out = match &rig.model.phase {
            Phase::Scanning(scan) => scan.out.clone(),
            _ => panic!("not scanning"),
        };
        // A file picked during the scan stays off the screen.
        assert!(rig.model.choose(rig.input.clone(), "again.png".into(), None, false).is_empty());
        let mut events = Vec::new();
        let places = rig.model.places.clone();
        calls::run(job, &places, &mut |msg| events.push(msg));
        let mut saw_face = false;
        for msg in events {
            let is_done = matches!(msg, Msg::ScanDone(_));
            rig.model.apply(msg);
            if !is_done {
                if let Phase::Scanning(scan) = &rig.model.phase {
                    saw_face |= !scan.faces.is_empty();
                    // Every crop that arrives is on disk and has a label.
                    for face in &scan.faces {
                        assert!(face.crop.as_ref().is_some_and(|c| c.is_file()));
                        assert!(face.accessible_label().starts_with("Face from frame 1."));
                    }
                }
            }
        }
        assert!(saw_face);
        let done = result(&rig.model);
        assert_eq!(done.out, out);
        assert_eq!(done.report.status, Status::Complete);
        assert_eq!(tone(&done.report), Tone::Candidate);
        assert!(done.report.headline.is(copy::POSSIBLE_CANDIDATE));
        assert_eq!(done.report.candidates.len(), 1);
        let card = &done.report.candidates[0];
        assert_eq!(card.poster.class, "missing");
        assert!(card.poster.image.as_ref().is_some_and(|p| PathBuf::from(p).is_file()));
        assert!(card.crop.as_ref().is_some_and(|c| out.join(c).is_file()));
        assert_eq!(headline_detail(&done.report), vec![Text::Shell("1 possible candidate to review.".into())]);
        // The small marker is seen and not compared.
        let others = other_faces(done);
        assert_eq!(others.len(), 1);
        assert_eq!(others[0].chip(), S::ChipNotCompared);
        // The leaving sheet only opens the poster's own fbi.gov page.
        let (title, url) = calls::leave(card.poster.fbi_url.as_deref().unwrap()).unwrap();
        assert!(title.is(copy::LEAVING));
        assert!(url.starts_with("https://www.fbi.gov/"));
        assert!(calls::leave("https://www.fbi.gov.evil.example/").unwrap_err().is(copy::ONLY_FBI));

        // Back returns to the estimate; Analyze again makes a new folder.
        assert_eq!(rig.model.back(), Back::Moved);
        assert!(matches!(rig.model.phase, Phase::Review));
        let again = rig.model.analyze(SystemTime::now()).unwrap();
        drive(&mut rig.model, vec![again]);
        assert_ne!(result(&rig.model).out, out);
    }

    #[test]
    fn wanted_only_is_a_clearance_line_not_a_candidate() {
        let mut rig = rig();
        choose(&mut rig);
        rig.model.set_classes(false, true);
        let job = rig.model.analyze(SystemTime::now()).unwrap();
        drive(&mut rig.model, vec![job]);
        let done = result(&rig.model);
        assert_eq!(tone(&done.report), Tone::Neutral);
        assert!(done.report.headline.is(copy::NO_CLEARANCE));
        let detail = headline_detail(&done.report);
        assert!(matches!(&detail[..], [Text::Core(line)] if line.code == "faces_some"));
    }

    #[test]
    fn both_classes_off_keeps_analyze_off() {
        let mut rig = rig();
        choose(&mut rig);
        rig.model.set_classes(false, false);
        assert_eq!(rig.model.analyze_block(), Some(Block::NoClass));
        assert!(rig.model.analyze(SystemTime::now()).is_none());
        assert!(matches!(rig.model.phase, Phase::Review));
        rig.model.set_classes(true, false);
        assert_eq!(rig.model.analyze_block(), None);
    }

    #[test]
    fn a_stop_is_incomplete_never_a_clearance() {
        let mut rig = rig();
        choose(&mut rig);
        let job = rig.model.analyze(SystemTime::now()).unwrap();
        assert_eq!(rig.model.back(), Back::AskStop);
        rig.model.stop();
        drive(&mut rig.model, vec![job]);
        let done = result(&rig.model);
        assert_eq!(done.report.status, Status::Incomplete);
        assert_eq!(tone(&done.report), Tone::Incomplete);
        assert!(done.report.headline.is(copy::INCOMPLETE));
        assert!(matches!(&headline_detail(&done.report)[0], Text::Core(line) if line.is(copy::CANCELLED)));
        assert!(!done.report.disclosure.iter().any(|l| l.is(copy::NO_CLEARANCE)));
    }

    #[test]
    fn refusals_show_their_sentence_and_what_to_do() {
        // No poster pack: the scan refuses.
        let root = tempfile::tempdir().unwrap();
        let mut model = Model::new(Places::new(root.path(), bundles()));
        let input = root.path().join("scene.png");
        openworld_core::fixture::demo_scene().image.save(&input).unwrap();
        let jobs = model.choose(input, "scene.png".into(), None, false);
        drive(&mut model, jobs);
        let job = model.analyze(SystemTime::now()).unwrap();
        drive(&mut model, vec![job]);
        let done = result(&model);
        assert_eq!(tone(&done.report), Tone::Error);
        assert!(done.report.headline.is(copy::R_PACK_MISSING));
        assert_eq!(headline_detail(&done.report), vec![Text::Shell(strings::text(S::HintInstallDemo).into())]);

        // A file that is not a picture: the estimate refuses and Analyze stays off.
        let mut rig = rig();
        let junk = rig.input.with_file_name("notes.png");
        std::fs::write(&junk, b"not a picture").unwrap();
        let jobs = rig.model.choose(junk, "notes.png".into(), None, false);
        drive(&mut rig.model, jobs);
        assert!(matches!(&rig.model.estimate, EstimateState::Refused(line) if line.is(copy::R_BAD_CODEC)));
        assert_eq!(rig.model.analyze_block(), Some(Block::Refused));
        assert_eq!(refusal_hint("bad_codec"), S::HintChooseFile);
        assert_eq!(refusal_hint("future_pack"), S::HintCheckClock);
        assert_eq!(refusal_hint("old_pack"), S::HintInstallDemo);
    }

    #[test]
    fn delete_removes_the_result_and_returns_to_start() {
        let mut rig = rig();
        choose(&mut rig);
        let job = rig.model.analyze(SystemTime::now()).unwrap();
        drive(&mut rig.model, vec![job]);
        let out = result(&rig.model).out.clone();
        assert!(out.is_dir());
        let job = rig.model.delete_result().unwrap();
        drive(&mut rig.model, vec![job]);
        assert!(!out.exists());
        assert!(matches!(rig.model.phase, Phase::Start));
        assert!(rig.model.file.is_none());
        // The original is never touched.
        assert!(rig.input.is_file());
    }

    #[test]
    fn closing_during_a_scan_stops_it_and_removes_its_folder() {
        let mut rig = rig();
        choose(&mut rig);
        let job = rig.model.analyze(SystemTime::now()).unwrap();
        let out = match &rig.model.phase {
            Phase::Scanning(scan) => scan.out.clone(),
            _ => unreachable!(),
        };
        assert!(!rig.model.close());
        assert!(!rig.model.can_quit);
        drive(&mut rig.model, vec![job]);
        assert!(rig.model.can_quit);
        assert!(!out.exists());
        assert!(matches!(rig.model.phase, Phase::Start));
    }

    #[test]
    fn options_re_estimate_and_stale_answers_are_dropped() {
        let mut rig = rig();
        choose(&mut rig);
        let stale = rig.model.set_coverage(Coverage::Measured).unwrap();
        let fresh = rig.model.set_bundle("accurate").unwrap();
        drive(&mut rig.model, vec![fresh, stale]);
        assert_eq!(rig.model.choice, Choice { bundle: "accurate".into(), coverage: Coverage::Measured, missing: true, wanted: true });
        let EstimateState::Ready(estimate) = &rig.model.estimate else { panic!() };
        assert!(estimate.coverage_banner.as_ref().is_some_and(|l| l.is(copy::BRIEF_FACE)));
        assert!(rig.model.set_bundle("accurate").is_none());
        // An imported copy is removed when the person moves on.
        let copy = rig.input.with_file_name("copy.png");
        std::fs::copy(&rig.input, &copy).unwrap();
        let jobs = rig.model.choose(copy.clone(), "copy.png".into(), None, true);
        drive(&mut rig.model, jobs);
        assert_eq!(rig.model.back(), Back::Moved);
        assert!(!copy.exists());
        assert!(matches!(rig.model.phase, Phase::Start));
    }
}
