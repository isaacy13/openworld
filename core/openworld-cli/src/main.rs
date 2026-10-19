// SPDX-License-Identifier: Apache-2.0

//! `openworld`: the scan library on the command line. Every command is one
//! library call; `--json` prints the library's own answer. Nothing is uploaded.

use clap::{Args, Parser, Subcommand};
use openworld_core::copy::{self, Line};
use openworld_core::engine::Event;
use openworld_core::estimate::{Coverage, Device};
use openworld_core::invoke::{self, Call};
use openworld_core::report::{ScanReport, Status, TrackStatus};
use openworld_core::scan::{scan_path, ScanRequest};
use serde_json::Value;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "openworld", version, about = "On-device scan of a photo or video you already have. Nothing is uploaded.")]
struct Cli {
    /// Print the library's JSON answer on stdout.
    #[arg(long, global = true)]
    json: bool,
    /// Bundle catalog: a folder of bundle folders.
    #[arg(long, global = true)]
    bundles: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone)]
struct Choices {
    #[arg(long, default_value = "fast")]
    bundle: String,
    /// complete (every decoded frame) or measured (5 a second, plus the tracker).
    #[arg(long, default_value = "complete")]
    coverage: String,
    /// Leave missing-person posters out.
    #[arg(long)]
    no_missing: bool,
    /// Leave wanted-person posters out.
    #[arg(long)]
    no_wanted: bool,
    /// Run on the CPU even when a GPU or neural engine registers.
    #[arg(long)]
    cpu_only: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// The bundle picker: name, best-for line, and whether a curve exists.
    Bundles,
    /// The time a scan will take, before Analyze.
    Estimate {
        #[arg(long)]
        input: PathBuf,
        #[command(flatten)]
        choices: Choices,
    },
    /// Scan a photo or video you already have.
    Scan {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        posters: PathBuf,
        /// A new or empty folder for the result. A folder that is not an OpenWorld result is never emptied.
        #[arg(long)]
        out: PathBuf,
        #[command(flatten)]
        choices: Choices,
        /// Scan events as JSON lines on stderr, while the scan runs.
        #[arg(long)]
        events: bool,
    },
    /// Guided: the file, what happens to it, the estimate, then a question before the scan starts.
    Analyze {
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        posters: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
        #[command(flatten)]
        choices: Choices,
        /// Start without asking.
        #[arg(long)]
        yes: bool,
    },
    /// Write a demo poster pack and a synthetic still, and scan it. No photograph of a person.
    Demo {
        #[arg(long)]
        out: PathBuf,
    },
    /// Paint a synthetic still for tests. No photograph of a person.
    FixtureStill {
        #[arg(long)]
        out: PathBuf,
        /// The demo scene: one marker to compare and one too small to.
        #[arg(long, conflicts_with_all = ["blank", "id"])]
        scene: bool,
        /// A canvas with no marker.
        #[arg(long, conflicts_with = "id")]
        blank: bool,
        /// One marker for this poster id.
        #[arg(long)]
        id: Option<u16>,
        /// Marker module in pixels; the marker is eight modules across.
        #[arg(long, default_value_t = 16)]
        module: u32,
    },
    /// Check the pipeline's rules end to end on demo markers. Not a face measurement.
    Selftest,
    /// Time the bundle on this device, per provider, and check scores against the CPU.
    Bench {
        #[arg(long, default_value = "fast")]
        bundle: String,
    },
    /// Delete a result folder. Only an OpenWorld result is deleted.
    Delete {
        #[arg(long)]
        out: PathBuf,
    },
    /// Delete expired results under a folder.
    Sweep {
        #[arg(long)]
        root: PathBuf,
    },
    /// Confirm leaving OpenWorld for an FBI page. Nothing is sent.
    Leave {
        #[arg(long)]
        url: String,
    },
    /// Poster packs.
    Posters {
        #[command(subcommand)]
        action: PosterCmd,
    },
    /// Every sentence the screens show, by code.
    Copy,
}

#[derive(Subcommand)]
enum PosterCmd {
    /// Check a pack: hash, age, and counts.
    Check {
        #[arg(long)]
        posters: PathBuf,
    },
    /// Fetch real posters. Stays off until an official curve allows it.
    Update,
    /// Write the demo pack.
    Demo {
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = run(cli).unwrap_or_else(|err| {
        eprintln!("{err}");
        1
    });
    std::process::exit(code);
}

/// The catalog: the flag, `OPENWORLD_BUNDLES`, or the nearest `bundles/` above here or above the program.
fn bundles_dir(flag: Option<PathBuf>) -> PathBuf {
    if let Some(path) = flag.or_else(|| std::env::var_os("OPENWORLD_BUNDLES").map(PathBuf::from)) {
        return path;
    }
    let starts = [std::env::current_dir().ok(), std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from))];
    for start in starts.into_iter().flatten() {
        let mut cursor = start;
        for _ in 0..6 {
            if cursor.join("bundles/fast/manifest.toml").is_file() {
                return cursor.join("bundles");
            }
            if !cursor.pop() {
                break;
            }
        }
    }
    PathBuf::from("bundles")
}

fn request(input: PathBuf, bundles: &Path, posters: PathBuf, out: PathBuf, choices: &Choices) -> Result<ScanRequest, Line> {
    let mut req = ScanRequest::new(input, bundles.to_path_buf(), posters, out);
    req.bundle = choices.bundle.clone();
    req.coverage = Coverage::parse(&choices.coverage)
        .ok_or_else(|| Line::with("bad_coverage", "Coverage must be complete or measured. Refusing.".into(), &[]))?;
    req.missing = !choices.no_missing;
    req.wanted = !choices.no_wanted;
    req.accelerate = !choices.cpu_only;
    req.device = Device::default();
    Ok(req)
}

fn run(cli: Cli) -> Result<i32, String> {
    let bundles = bundles_dir(cli.bundles.clone());
    let json_mode = cli.json;
    let call = |c: Call| invoke::run(c, None, &mut |_| {});
    match cli.cmd {
        Cmd::Bundles => answer(json_mode, call(Call::Bundles { bundles }), bundle_lines),
        Cmd::Copy => answer(json_mode, call(Call::Copy {}), |v| {
            v["copy"].as_object().map(|m| m.iter().map(|(k, t)| format!("{k}: {}", t.as_str().unwrap_or(""))).collect()).unwrap_or_default()
        }),
        Cmd::Leave { url } => answer(json_mode, call(Call::Leave { url }), |v| vec![text(&v["message"]), v["url"].as_str().unwrap_or("").into()]),
        Cmd::Delete { out } => answer(json_mode, call(Call::Delete { out }), |v| vec![text(&v["headline"])]),
        Cmd::Sweep { root } => answer(json_mode, call(Call::Sweep { root }), |v| vec![format!("Removed {} expired results.", v["removed"])]),
        Cmd::Posters { action } => {
            let value = match action {
                PosterCmd::Check { posters } => call(Call::PostersCheck { posters }),
                PosterCmd::Update => call(Call::PostersUpdate { bundles }),
                PosterCmd::Demo { out } => call(Call::PostersDemo { out }),
            };
            answer(json_mode, value, |v| {
                let p = &v["posters"];
                vec![
                    format!("Pack {} from {}, fetched {}.", p["id"].as_str().unwrap_or(""), p["source"].as_str().unwrap_or(""), p["created_at"].as_str().unwrap_or("")),
                    format!("{} missing, {} wanted.", p["missing"], p["wanted"]),
                    format!("SHA-256 {}", p["sha256"].as_str().unwrap_or("")),
                ]
            })
        }
        Cmd::Estimate { input, choices } => {
            let req = match request(input, &bundles, PathBuf::new(), PathBuf::new(), &choices) {
                Ok(req) => req,
                Err(line) => return answer(json_mode, invoke::refusal(line), |_| vec![]),
            };
            answer(json_mode, call(Call::Estimate(req)), estimate_lines)
        }
        Cmd::Scan { input, posters, out, choices, events } => {
            let req = match request(input, &bundles, posters, out, &choices) {
                Ok(req) => req,
                Err(line) => return answer(json_mode, invoke::refusal(line), |_| vec![]),
            };
            scan(json_mode, events, &req)
        }
        Cmd::Analyze { input, posters, out, choices, yes } => analyze(json_mode, &bundles, input, posters, out, &choices, yes),
        Cmd::Demo { out } => demo(json_mode, &bundles, &out),
        Cmd::FixtureStill { out, scene, blank, id, module } => fixture_still(json_mode, &out, scene, blank, id, module),
        Cmd::Selftest => selftest(json_mode, &bundles),
        Cmd::Bench { bundle } => bench(json_mode, &bundles, &bundle),
    }
}

fn text(value: &Value) -> String {
    value["text"].as_str().unwrap_or("").to_string()
}

fn print_lines(lines: impl IntoIterator<Item = String>) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
}

/// Print the answer. A refusal prints its sentence and exits 2.
fn answer(json_mode: bool, value: Value, lines: impl Fn(&Value) -> Vec<String>) -> Result<i32, String> {
    let refused = value["ok"] == false;
    if json_mode {
        print_lines([value.to_string()]);
    } else if refused {
        print_lines([text(&value["headline"])]);
    } else {
        print_lines(lines(&value));
    }
    Ok(if refused { 2 } else { 0 })
}

fn bundle_lines(value: &Value) -> Vec<String> {
    let mut lines = vec!["Model bundle".to_string()];
    for row in value["bundles"].as_array().into_iter().flatten() {
        let name = row["name"].as_str().unwrap_or("");
        let selected = if row["preselected"] == true { " Selected." } else { "" };
        lines.push(format!("{name}.{selected} {} {}", row["best_for"].as_str().unwrap_or(""), text(&row["curve_line"])));
    }
    lines.push(text(&value["note"]));
    lines
}

fn estimate_lines(value: &Value) -> Vec<String> {
    let mut lines = vec![
        "Estimate".to_string(),
        format!("{}.", text(&value["human"])),
        text(&value["detection_line"]),
        text(&value["coverage_line"]),
    ];
    if value["coverage_banner"].is_object() {
        lines.push(text(&value["coverage_banner"]));
    }
    for note in value["notes"].as_array().into_iter().flatten() {
        lines.push(text(note));
    }
    lines.push(text(&value["classes"]));
    lines.push(text(&value["caveat"]));
    lines
}

fn event_line(event: &Event) -> Option<String> {
    match event {
        Event::Face { frame_label, crop, .. } => Some(format!("Face. {} {}", frame_label.text, crop.clone().unwrap_or_default())),
        Event::Track { track } => Some(format!("{} {} {}", track.frames.text, track.label.text, track.reason.as_ref().map(|r| r.text.clone()).unwrap_or_default())),
        Event::Progress { .. } => None,
    }
}

fn scan(json_mode: bool, events: bool, req: &ScanRequest) -> Result<i32, String> {
    if !(req.missing || req.wanted) {
        return answer(json_mode, invoke::refusal(Line::of(copy::CHOOSE_CLASS)), |_| vec![]);
    }
    if !json_mode {
        print_lines(["Scanning".to_string()]);
    }
    let stderr = io::stderr();
    let report = scan_path(req, None, &mut |event| {
        if events {
            let _ = writeln!(stderr.lock(), "{}", serde_json::to_string(&event).unwrap_or_default());
        } else if !json_mode {
            if let Some(line) = event_line(&event) {
                print_lines([line]);
            }
        }
    });
    finish(json_mode, &report)
}

fn finish(json_mode: bool, report: &ScanReport) -> Result<i32, String> {
    let mut value = serde_json::to_value(report).map_err(|e| e.to_string())?;
    value["ok"] = (!report.is_refused()).into();
    answer(json_mode, value, |_| report_lines(report))?;
    Ok(match report.status {
        Status::Complete => 0,
        Status::Incomplete => 3,
        Status::Refused => 2,
    })
}

fn report_lines(report: &ScanReport) -> Vec<String> {
    let mut lines = vec![report.headline.text.clone()];
    if let Some(reason) = &report.reason {
        lines.push(reason.text.clone());
    }
    for line in [&report.perception_note, &report.frames_line, &report.faces_line, &report.classes, &report.coverage_line, &report.coverage_banner, &report.detection_line, &report.device_note, &report.chance].into_iter().flatten() {
        lines.push(line.text.clone());
    }
    lines.push(format!("Bundle: {} {}. Posters: {} ({}).", report.bundle.name, report.bundle.version, report.posters.id, report.posters.sha256));
    lines.extend(report.warnings.iter().map(|w| w.text.clone()));
    for card in &report.candidates {
        lines.push(String::new());
        lines.push(card.headline.text.clone());
        lines.push(format!("{} {} ({})", card.frame_label.text, card.poster.title, card.poster.class_label));
        lines.push(card.band.text.clone());
        lines.extend(card.crop.iter().map(|c| format!("Crop {c}")));
        lines.extend(card.frame.iter().map(|f| format!("Frame {f}")));
        if let Some(url) = &card.poster.fbi_url {
            lines.push(format!("Open FBI page {url}"));
            lines.push(card.leaving.text.clone());
            lines.extend(card.leave_notes.iter().map(|n| n.text.clone()));
        }
    }
    let others: Vec<_> = report.tracks.iter().filter(|t| t.status != TrackStatus::Candidate).collect();
    if !others.is_empty() {
        lines.push(String::new());
        lines.push("Other faces in this file.".into());
        for track in others {
            let why = track.reason.as_ref().map(|r| format!(" {}", r.text)).unwrap_or_default();
            lines.push(format!("{} {}{why} {}", track.frames.text, track.label.text, track.crop.clone().unwrap_or_default()));
        }
    }
    lines.push(String::new());
    lines.extend(report.disclosure.iter().map(|l| l.text.clone()));
    lines
}

fn ask(question: &str) -> Result<bool, String> {
    let mut out = io::stdout().lock();
    write!(out, "{question} [y/N] ").and_then(|_| out.flush()).map_err(|e| e.to_string())?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer).map_err(|e| e.to_string())?;
    if !io::stdin().is_terminal() {
        writeln!(out).map_err(|e| e.to_string())?;
    }
    Ok(matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn ask_path(question: &str) -> Result<PathBuf, String> {
    let mut out = io::stdout().lock();
    write!(out, "{question} ").and_then(|_| out.flush()).map_err(|e| e.to_string())?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer).map_err(|e| e.to_string())?;
    Ok(PathBuf::from(answer.trim()))
}

fn analyze(json_mode: bool, bundles: &Path, input: Option<PathBuf>, posters: Option<PathBuf>, out: Option<PathBuf>, choices: &Choices, yes: bool) -> Result<i32, String> {
    if !json_mode {
        print_lines([copy::NO_CAMERA.1.to_string()]);
    }
    let input = match input {
        Some(path) => path,
        None => ask_path("File:")?,
    };
    let posters = match posters {
        Some(path) => path,
        None => ask_path("Poster pack:")?,
    };
    let out = match out {
        Some(path) => path,
        None => ask_path("Result folder (new or empty):")?,
    };
    let req = match request(input, bundles, posters, out, choices) {
        Ok(req) => req,
        Err(line) => return answer(json_mode, invoke::refusal(line), |_| vec![]),
    };
    let estimate = invoke::run(Call::Estimate(req.clone()), None, &mut |_| {});
    if estimate["ok"] == false || estimate["can_start"] == false {
        let refusal = if estimate["ok"] == false { estimate } else { invoke::refusal(Line::of(copy::CHOOSE_CLASS)) };
        return answer(json_mode, refusal, |_| vec![]);
    }
    if !json_mode {
        print_lines(copy::DISCLOSURE.iter().map(|line| line.1.to_string()));
        print_lines(estimate_lines(&estimate));
    }
    if !yes && !ask("Analyze on this device?")? {
        if !json_mode {
            print_lines(["Not started.".to_string()]);
        }
        return Ok(0);
    }
    scan(json_mode, false, &req)
}

#[cfg(feature = "fixture")]
fn demo(json_mode: bool, bundles: &Path, out: &Path) -> Result<i32, String> {
    if openworld_core::outdir::creation_blocked(out) || (out.exists() && std::fs::read_dir(out).map(|mut d| d.next().is_some()).unwrap_or(true)) {
        return answer(json_mode, invoke::refusal(Line::of(copy::R_OUT_FOREIGN)), |_| vec![]);
    }
    let posters = out.join("posters");
    if let Err(line) = openworld_core::posters::write_fixture_pack(&posters, std::time::SystemTime::now()) {
        return answer(json_mode, invoke::refusal(line), |_| vec![]);
    }
    let input = out.join("input.png");
    if openworld_core::fixture::demo_scene().image.save(&input).is_err() {
        return answer(json_mode, invoke::refusal(Line::with("still_unwritten", "The still could not be written. Refusing.".into(), &[])), |_| vec![]);
    }
    let req = ScanRequest::new(input, bundles.to_path_buf(), posters, out.join("result"));
    scan(json_mode, false, &req)
}

#[cfg(feature = "fixture")]
fn fixture_still(json_mode: bool, out: &Path, scene: bool, blank: bool, id: Option<u16>, module: u32) -> Result<i32, String> {
    use openworld_core::fixture;
    let image = if scene {
        fixture::demo_scene().image
    } else {
        let mut canvas = openworld_core::geom::blank(640, 480);
        if !blank {
            let id = id.unwrap_or(7);
            let marker = fixture::render(id, module);
            let (x, y) = fixture::passing_origin(id, marker.width(), marker.height());
            fixture::place(&mut canvas, &marker, x, y);
        }
        canvas
    };
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        if std::fs::create_dir_all(parent).is_err() {
            return answer(json_mode, invoke::refusal(Line::of(copy::R_OUT_CREATE)), |_| vec![]);
        }
    }
    match image.save(out) {
        Ok(()) => answer(json_mode, serde_json::json!({ "ok": true, "still": out }), |_| vec![out.display().to_string()]),
        Err(_) => answer(json_mode, invoke::refusal(Line::with("still_unwritten", "The still could not be written. Refusing.".into(), &[])), |_| vec![]),
    }
}

#[cfg(feature = "fixture")]
fn selftest(json_mode: bool, bundles: &Path) -> Result<i32, String> {
    let bundle = match openworld_core::load_bundles(bundles).map(|all| all.into_iter().find(|b| b.id == "fast")) {
        Ok(Some(bundle)) => bundle,
        Ok(None) => return answer(json_mode, invoke::refusal(Line::of(copy::R_NO_BUNDLE)), |_| vec![]),
        Err(line) => return answer(json_mode, invoke::refusal(line), |_| vec![]),
    };
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let pack = openworld_core::posters::write_fixture_pack(dir.path(), std::time::SystemTime::now()).map_err(|l| l.text)?;
    let result = openworld_core::measure::self_test(&bundle, &pack);
    let value = serde_json::to_value(&result).map_err(|e| e.to_string())?;
    answer(json_mode, value.clone(), |v| vec![serde_json::to_string_pretty(v).unwrap_or_default()])
}

#[cfg(not(feature = "fixture"))]
fn demo(json_mode: bool, _bundles: &Path, _out: &Path) -> Result<i32, String> {
    answer(json_mode, invoke::refusal(Line::of(copy::R_FIXTURE_OFF)), |_| vec![])
}

#[cfg(not(feature = "fixture"))]
fn fixture_still(json_mode: bool, _out: &Path, _scene: bool, _blank: bool, _id: Option<u16>, _module: u32) -> Result<i32, String> {
    answer(json_mode, invoke::refusal(Line::of(copy::R_FIXTURE_OFF)), |_| vec![])
}

#[cfg(not(feature = "fixture"))]
fn selftest(json_mode: bool, _bundles: &Path) -> Result<i32, String> {
    answer(json_mode, invoke::refusal(Line::of(copy::R_FIXTURE_OFF)), |_| vec![])
}

fn bench(json_mode: bool, bundles: &Path, id: &str) -> Result<i32, String> {
    let bundle = match openworld_core::load_bundles(bundles).map(|all| all.into_iter().find(|b| b.id == id)) {
        Ok(Some(bundle)) => bundle,
        Ok(None) => return answer(json_mode, invoke::refusal(Line::of(copy::R_NO_BUNDLE)), |_| vec![]),
        Err(line) => return answer(json_mode, invoke::refusal(line), |_| vec![]),
    };
    match openworld_core::bench::bench(&bundle) {
        Ok(result) => {
            let value = serde_json::to_value(&result).map_err(|e| e.to_string())?;
            answer(json_mode, value, |v| vec![serde_json::to_string_pretty(v).unwrap_or_default()])
        }
        Err(message) => answer(json_mode, invoke::refusal(Line::with(copy::R_NO_WEIGHTS.0, message, &[])), |_| vec![]),
    }
}
