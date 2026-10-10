// SPDX-License-Identifier: Apache-2.0

use clap::{Args, Parser, Subcommand};
use openworld_core::copy::product_copy;
use openworld_core::embed::fixture_probe;
use openworld_core::estimate::{Coverage, DetectionSize, FormFactor};
use openworld_core::fiducial::{self, render_face_module};
use openworld_core::geom::blank;
use openworld_core::hardware::resolve_execution;
use openworld_core::posters::write_fixture_pack;
use openworld_core::scan::{
    delete_output, estimate_for, leave_prompt, scan_path, MediaFacts, Progress, ScanRequest,
};
use openworld_core::scene::demo_scene;
use openworld_core::{load_bundles, measure_fast};
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Parser)]
#[command(name = "openworld", about = "On-device scan. Nothing is uploaded.")]
struct Cli {
    /// Print one JSON document on stdout.
    #[arg(long, global = true)]
    json: bool,
    /// Bundle catalog. A folder of bundle folders.
    #[arg(long, global = true)]
    bundles: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// The words the screens use.
    Copy,
    /// Name, best-for line, and whether a curve exists.
    Bundles,
    /// Time, and on a phone heat and battery, before Analyze.
    Estimate {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = "fast")]
        bundle: String,
        /// 320, 480, 640, or full. Smaller starts selected in the prompts.
        #[arg(long, default_value = "640")]
        long_side: String,
        #[arg(long, default_value = "complete")]
        coverage: String,
        #[arg(long, default_value = "computer")]
        form_factor: String,
        #[arg(long, default_value = "cpu")]
        provider: String,
        /// Leave missing posters out. The estimate then says "Wanted."
        #[arg(long)]
        no_missing: bool,
        /// Leave wanted posters out. The estimate then says "Missing."
        #[arg(long)]
        no_wanted: bool,
        #[command(flatten)]
        media: PlatformMedia,
    },
    /// Scan a photo or video the user already has.
    Scan {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = "fast")]
        bundle: String,
        #[arg(long, default_value = "640")]
        long_side: String,
        #[arg(long, default_value = "complete")]
        coverage: String,
        #[arg(long)]
        posters: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "computer")]
        form_factor: String,
        #[arg(long, default_value = "cpu")]
        provider: String,
        #[arg(long, default_value_t = true)]
        missing: bool,
        #[arg(long, default_value_t = true)]
        wanted: bool,
        #[arg(long)]
        no_missing: bool,
        #[arg(long)]
        no_wanted: bool,
        #[arg(long)]
        abort_after_frames: Option<u64>,
        /// Frames already decoded by AVFoundation or MediaCodec. Skips FFmpeg.
        #[arg(long)]
        frames: Option<PathBuf>,
        #[command(flatten)]
        media: PlatformMedia,
        /// JSON progress lines on stderr, including face crops as they are written.
        #[arg(long)]
        progress: bool,
    },
    /// Prompts: file, on-device disclosure, bundle, frame size, estimate, then scan.
    Analyze {
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        bundle: Option<String>,
        #[arg(long)]
        long_side: Option<String>,
        #[arg(long)]
        coverage: Option<String>,
        #[arg(long)]
        posters: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value = "computer")]
        form_factor: String,
        /// Leave missing posters out. The estimate then says "Wanted."
        #[arg(long)]
        no_missing: bool,
        /// Leave wanted posters out. The estimate then says "Missing."
        #[arg(long)]
        no_wanted: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Synthetic fixture scan. No photograph of a person.
    Demo {
        #[arg(long)]
        out: PathBuf,
    },
    /// Paint a synthetic still. No photograph of a person.
    FixtureStill {
        #[arg(long)]
        out: PathBuf,
        /// Compared face, small face, vehicle, and plate.
        #[arg(long)]
        scene: bool,
        /// A canvas with no marker.
        #[arg(long)]
        blank: bool,
        #[arg(long, default_value_t = 7)]
        id: u16,
        /// Pixel size of one marker module.
        #[arg(long, default_value_t = 16)]
        module: u32,
        #[arg(long)]
        x: Option<u32>,
        #[arg(long)]
        y: Option<u32>,
        /// Place the marker where the fixture score is below the Fast cutoff.
        #[arg(long)]
        below_cutoff: bool,
    },
    /// Delete a result directory.
    Delete {
        #[arg(long)]
        out: PathBuf,
    },
    /// Confirm leaving OpenWorld for an FBI page. Does not send a report.
    Leave {
        #[arg(long)]
        url: String,
    },
    /// Measure the Fast fixture curve. Prints counts. Does not allow real photos.
    Measure,
    /// Poster pack checks. Update stays off until a real curve allows it.
    Posters {
        #[command(subcommand)]
        action: PosterCmd,
    },
}

#[derive(Subcommand)]
enum PosterCmd {
    /// Load a pack and print its id, or the refusal.
    Check {
        #[arg(long)]
        posters: PathBuf,
    },
    /// Would download api.fbi.gov. Refuses while real photos are off.
    Update,
    /// Write the synthetic poster pack. No FBI fetch and no real faces.
    WriteFixture {
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Args, Clone)]
struct PlatformMedia {
    /// From the platform decoder. When set, the file is not probed with FFmpeg.
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    #[arg(long)]
    fps: Option<f64>,
    #[arg(long)]
    frame_count: Option<u64>,
    #[arg(long)]
    duration: Option<f64>,
    /// Container creation time, seconds since the Unix epoch.
    #[arg(long)]
    container_unix: Option<u64>,
    /// The file is a video. Stills omit this.
    #[arg(long)]
    video: bool,
}

impl PlatformMedia {
    fn facts(&self) -> Result<Option<MediaFacts>, String> {
        let untouched = self.width.is_none()
            && self.height.is_none()
            && self.frame_count.is_none()
            && self.fps.is_none()
            && self.duration.is_none()
            && self.container_unix.is_none()
            && !self.video;
        if untouched {
            return Ok(None);
        }
        let width = self.width.ok_or("A platform decode needs a width.")?;
        let height = self.height.ok_or("A platform decode needs a height.")?;
        let frames = self
            .frame_count
            .ok_or("A platform decode needs a frame count.")?;
        if width == 0 || height == 0 || frames == 0 {
            return Err("Bad codec or unreadable file. Refusing.".into());
        }
        Ok(Some(MediaFacts {
            width,
            height,
            fps: self.fps.unwrap_or(0.0),
            frames,
            duration_sec: self.duration.unwrap_or(0.0),
            video: self.video,
            container_unix: self.container_unix,
        }))
    }
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> Result<i32, String> {
    let bundles = bundles_dir(cli.bundles.clone());
    match cli.cmd {
        Cmd::Copy => {
            let words = product_copy();
            emit(cli.json, words.clone());
            speak(cli.json, copy_lines(&words));
            Ok(0)
        }
        Cmd::Bundles => {
            let all = load_bundles(&bundles).map_err(|e| e.to_string())?;
            let rows: Vec<_> = all.iter().map(openworld_core::bundle::row_json).collect();
            emit(cli.json, json!({ "bundles": rows }));
            speak(cli.json, bundle_lines(&rows));
            Ok(0)
        }
        Cmd::Estimate {
            input,
            bundle,
            long_side,
            coverage,
            form_factor,
            provider,
            no_missing,
            no_wanted,
            media,
        } => {
            let req = request(
                &bundles,
                &input,
                &bundle,
                &long_side,
                &coverage,
                &form_factor,
                &provider,
                PathBuf::from("."),
                PathBuf::from("."),
                !no_missing,
                !no_wanted,
                None,
                None,
                media.facts()?,
            )?;
            match estimate_for(&req) {
                Ok(est) => {
                    emit(
                        cli.json,
                        serde_json::to_value(&est).map_err(|e| e.to_string())?,
                    );
                    if !cli.json {
                        let name = bundle_display_name(&bundles, &req.bundle_id);
                        for line in estimate_lines(
                            &est,
                            &name,
                            req.detection,
                            req.coverage,
                            req.missing,
                            req.wanted,
                            file_is_old(&input),
                        ) {
                            println!("{line}");
                        }
                    }
                    Ok(0)
                }
                Err(report) => finish_report(cli.json, &report),
            }
        }
        Cmd::Scan {
            input,
            bundle,
            long_side,
            coverage,
            posters,
            out,
            form_factor,
            provider,
            missing,
            wanted,
            no_missing,
            no_wanted,
            abort_after_frames,
            frames,
            media,
            progress,
        } => {
            let req = request(
                &bundles,
                &input,
                &bundle,
                &long_side,
                &coverage,
                &form_factor,
                &provider,
                posters,
                out,
                missing && !no_missing,
                wanted && !no_wanted,
                abort_after_frames,
                frames,
                media.facts()?,
            )?;
            scan_and_report(cli.json, progress, &req)
        }
        Cmd::Analyze {
            input,
            bundle,
            long_side,
            coverage,
            posters,
            out,
            form_factor,
            no_missing,
            no_wanted,
            yes,
        } => analyze(
            cli.json,
            &bundles,
            input,
            bundle,
            long_side,
            coverage,
            posters,
            out,
            &form_factor,
            no_missing,
            no_wanted,
            yes,
        ),
        Cmd::Demo { out } => demo(&bundles, &out, cli.json),
        Cmd::FixtureStill {
            out,
            scene,
            blank,
            id,
            module,
            x,
            y,
            below_cutoff,
        } => fixture_still(
            &bundles,
            &out,
            scene,
            blank,
            id,
            module,
            x,
            y,
            below_cutoff,
            cli.json,
        ),
        Cmd::Delete { out } => match delete_output(&out) {
            Ok(()) => {
                emit(cli.json, json!({ "deleted": true }));
                speak(cli.json, ["Deleted."]);
                Ok(0)
            }
            Err(err) => {
                emit(cli.json, json!({ "deleted": false, "message": &err }));
                speak(cli.json, [err.as_str()]);
                Ok(2)
            }
        },
        Cmd::Leave { url } => match leave_prompt(&url) {
            Ok(prompt) => {
                emit(
                    cli.json,
                    serde_json::to_value(&prompt).map_err(|e| e.to_string())?,
                );
                if !cli.json {
                    println!("{}", prompt.message);
                    println!("{}", prompt.url);
                }
                Ok(0)
            }
            Err(err) => {
                emit(cli.json, json!({ "message": &err }));
                speak(cli.json, [err.as_str()]);
                Ok(2)
            }
        },
        Cmd::Measure => {
            let all = load_bundles(&bundles).map_err(|e| e.to_string())?;
            let fast = all
                .into_iter()
                .find(|b| b.id == "fast")
                .ok_or("Fast bundle is missing.")?;
            let measurement = measure_fast(&fast);
            emit(
                true,
                serde_json::to_value(&measurement).map_err(|e| e.to_string())?,
            );
            Ok(0)
        }
        Cmd::Posters { action } => match action {
            PosterCmd::Check { posters } => {
                match openworld_core::load_pack(&posters, SystemTime::now()) {
                    Ok(pack) => {
                        emit(
                            cli.json,
                            json!({
                                "id": pack.id,
                                "posters": pack.posters.len(),
                                "perception": pack.perception,
                                "expires_at": pack.expires_at,
                            }),
                        );
                        speak(cli.json, poster_check_lines(&pack));
                        Ok(0)
                    }
                    Err(err) => {
                        let message = err.refusal();
                        emit(cli.json, json!({ "status": "refused", "message": message }));
                        speak(cli.json, [message]);
                        Ok(2)
                    }
                }
            }
            PosterCmd::WriteFixture { out } => {
                let pack =
                    write_fixture_pack(&out, SystemTime::now()).map_err(|e| e.to_string())?;
                emit(
                    cli.json,
                    json!({
                        "id": pack.id,
                        "posters": pack.posters.len(),
                        "perception": pack.perception,
                        "note": "Fixture posters. Real FBI photos stay off.",
                    }),
                );
                speak(cli.json, ["Fixture posters. Real FBI photos stay off."]);
                Ok(0)
            }
            PosterCmd::Update => {
                let allowed = load_bundles(&bundles)
                    .ok()
                    .and_then(|all| all.into_iter().find(|b| b.id == "fast"))
                    .map(|b| b.real_posters_allowed)
                    .unwrap_or(false);
                match openworld_core::update::update_posters(allowed, &mut RefuseTransport) {
                    Ok(_) => Ok(0),
                    Err(err) => {
                        let message = "Real FBI photos stay off. Fast does not have a curve that allows them.";
                        emit(
                            cli.json,
                            json!({ "status": "refused", "message": message, "detail": err.to_string() }),
                        );
                        if !cli.json {
                            eprintln!("{message}");
                        }
                        Ok(2)
                    }
                }
            }
        },
    }
}

struct RefuseTransport;
impl openworld_core::update::Transport for RefuseTransport {
    fn get(&mut self, _url: &str) -> Result<Vec<u8>, String> {
        Err("real FBI photos stay off".into())
    }
}

fn analyze(
    json_mode: bool,
    bundles: &Path,
    input: Option<PathBuf>,
    bundle: Option<String>,
    long_side: Option<String>,
    coverage: Option<String>,
    posters: Option<PathBuf>,
    out: Option<PathBuf>,
    form_factor: &str,
    no_missing: bool,
    no_wanted: bool,
    yes: bool,
) -> Result<i32, String> {
    let input = match input {
        Some(path) => path,
        None => {
            if !json_mode {
                for line in choose_lines() {
                    println!("{line}");
                }
            }
            PathBuf::from(prompt("Photo or video path:", json_mode)?)
        }
    };
    speak(json_mode, device_lines(&input));
    let all = load_bundles(bundles).map_err(|e| e.to_string())?;
    let bundle = match bundle {
        Some(id) => id,
        None => {
            if !json_mode {
                for line in bundle_menu_lines(&all) {
                    println!("{line}");
                }
            }
            let entered = prompt("Bundle [fast]:", json_mode)?;
            if entered.is_empty() {
                "fast".into()
            } else {
                entered
            }
        }
    };
    let long_side = match long_side {
        Some(value) => value,
        None => {
            if !json_mode {
                for line in size_menu_lines() {
                    println!("{line}");
                }
            }
            let entered = prompt("Detection long side (320, 480, 640, full) [640]:", json_mode)?;
            if entered.is_empty() {
                "640".into()
            } else {
                entered
            }
        }
    };
    let coverage = match coverage {
        Some(value) => value,
        None => {
            if !json_mode {
                println!("Coverage:");
                for (id, label) in [
                    ("complete", "Complete. Every decoded frame."),
                    ("measured", "Measured. 5 frames a second, plus the tracker."),
                ] {
                    println!("  {id} — {}", marked_name(label, id == "complete"));
                }
            }
            let entered = prompt("Coverage (complete, measured) [complete]:", json_mode)?;
            if entered.is_empty() {
                "complete".into()
            } else {
                entered
            }
        }
    };
    let posters = match posters {
        Some(path) => path,
        None => PathBuf::from(prompt("Poster pack directory:", json_mode)?),
    };
    let out = match out {
        Some(path) => path,
        None => PathBuf::from(prompt("Result directory:", json_mode)?),
    };
    let (missing, wanted) = if yes || no_missing || no_wanted {
        (!no_missing, !no_wanted)
    } else {
        if !json_mode {
            for line in class_menu_lines() {
                println!("{line}");
            }
        }
        (prompt_on("Missing", json_mode)?, prompt_on("Wanted", json_mode)?)
    };
    let req = request(
        bundles,
        &input,
        &bundle,
        &long_side,
        &coverage,
        form_factor,
        "cpu",
        posters,
        out,
        missing,
        wanted,
        None,
        None,
        None,
    )?;
    match estimate_for(&req) {
        Ok(est) => {
            let name = all
                .iter()
                .find(|item| item.id == req.bundle_id)
                .map(|item| item.name.as_str())
                .unwrap_or(req.bundle_id.as_str());
            speak(
                json_mode,
                estimate_lines(
                    &est,
                    name,
                    req.detection,
                    req.coverage,
                    req.missing,
                    req.wanted,
                    file_is_old(&input),
                ),
            );
        }
        Err(report) => return finish_report(json_mode, &report),
    }
    if !req.missing && !req.wanted {
        emit(
            json_mode,
            json!({
                "status": "refused",
                "message": openworld_core::copy::CHOOSE_CLASS,
            }),
        );
        return Ok(2);
    }
    if !yes {
        let answer = prompt("Analyze? [y/N]:", json_mode)?;
        if !matches!(answer.as_str(), "y" | "Y" | "yes") {
            emit(
                json_mode,
                json!({ "started": false, "message": "Not started." }),
            );
            speak(json_mode, ["Not started."]);
            return Ok(0);
        }
    }
    scan_and_report(json_mode, false, &req)
}

fn fixture_still(
    bundles: &Path,
    out: &Path,
    scene: bool,
    blank_canvas: bool,
    id: u16,
    module: u32,
    x: Option<u32>,
    y: Option<u32>,
    below_cutoff: bool,
    json_mode: bool,
) -> Result<i32, String> {
    if scene && blank_canvas {
        return Err("Choose a scene or a blank still.".into());
    }
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    if scene {
        let threshold = fast_threshold(bundles)?;
        let layout = demo_scene(threshold);
        layout.image.save(out).map_err(|e| e.to_string())?;
        emit(
            json_mode,
            json!({
                "path": out,
                "scene": true,
                "id": layout.compared_id,
                "width": layout.image.width(),
                "height": layout.image.height(),
            }),
        );
        return Ok(0);
    }
    if blank_canvas {
        let image = blank(400, 320);
        image.save(out).map_err(|e| e.to_string())?;
        emit(
            json_mode,
            json!({ "path": out, "blank": true, "width": 400, "height": 320 }),
        );
        return Ok(0);
    }
    if module == 0 {
        return Err("Module size must be at least 1.".into());
    }
    let threshold = fast_threshold(bundles)?;
    let marker = render_face_module(id, module);
    let (mw, mh) = marker.dimensions();
    let (px, py) = match (x, y, below_cutoff) {
        (Some(_), Some(_), true) => {
            return Err("A below-cutoff still picks its own origin.".into());
        }
        (Some(x), Some(y), false) => (x, y),
        (None, None, below) => find_origin(id, mw, mh, threshold, below)?,
        _ => return Err("Provide both x and y, or neither.".into()),
    };
    if px.saturating_add(mw) > 400 || py.saturating_add(mh) > 320 {
        return Err("The marker does not fit on the still.".into());
    }
    let mut image = blank(400, 320);
    fiducial::place(&mut image, &marker, px, py);
    image.save(out).map_err(|e| e.to_string())?;
    emit(
        json_mode,
        json!({
            "path": out,
            "id": id,
            "module": module,
            "x": px,
            "y": py,
            "width": 400,
            "height": 320,
            "below_cutoff": below_cutoff,
        }),
    );
    Ok(0)
}

fn find_origin(id: u16, w: u32, h: u32, threshold: f32, below: bool) -> Result<(u32, u32), String> {
    for y in (8..80).step_by(2) {
        for x in (8..80).step_by(2) {
            let (_probe, score) = fixture_probe(id, x, y, w, h, 0);
            let hit = if below {
                score < threshold
            } else {
                score >= threshold
            };
            if hit {
                return Ok((x, y));
            }
        }
    }
    if below {
        Err("No placement scored below the locked cutoff.".into())
    } else {
        Err("No placement scored at or above the locked cutoff.".into())
    }
}

fn fast_threshold(bundles: &Path) -> Result<f32, String> {
    load_bundles(bundles)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|b| b.id == "fast")
        .map(|b| b.threshold)
        .ok_or_else(|| "Fast bundle is missing.".into())
}

fn demo(bundles: &Path, out: &Path, json_mode: bool) -> Result<i32, String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let posters = out.join("posters");
    let pack = write_fixture_pack(&posters, SystemTime::now()).map_err(|e| e.to_string())?;
    let fast = load_bundles(bundles)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|b| b.id == "fast")
        .ok_or("Fast bundle is missing.")?;
    let scene = demo_scene(fast.threshold);
    let input = out.join("input.png");
    scene.image.save(&input).map_err(|e| e.to_string())?;
    let result = out.join("result");
    let req = request(
        bundles, &input, "fast", "640", "complete", "computer", "cpu", posters, result, true, true,
        None, None, None,
    )?;
    let _ = pack;
    scan_and_report(json_mode, false, &req)
}

/// Say Scanning, then name each crop the way the screen does, then the report.
fn scan_and_report(json_mode: bool, progress_json: bool, req: &ScanRequest) -> Result<i32, String> {
    if !json_mode {
        println!("Scanning");
    }
    let mut named = false;
    let report = scan_path(req, &mut |event: Progress| {
        if progress_json {
            if let Ok(line) = serde_json::to_string(&event) {
                eprintln!("{line}");
            }
        }
        if json_mode {
            return;
        }
        let Some(lines) = live_crop_lines(&event) else {
            return;
        };
        if !named {
            println!("Crops from this file.");
            named = true;
        }
        for line in lines {
            println!("{line}");
        }
    });
    finish_report(json_mode, &report)
}

/// The crop a screen shows while Scanning. The path stands in for the picture.
fn live_crop_lines(event: &Progress) -> Option<Vec<String>> {
    if !matches!(event.kind.as_str(), "face" | "plate" | "vehicle") || event.label.is_empty() {
        return None;
    }
    let crop = event.crop.as_ref().filter(|path| !path.is_empty())?;
    let mut lines = Vec::new();
    if !event.frame_label.is_empty() {
        lines.push(event.frame_label.clone());
    }
    lines.push(event.label.clone());
    lines.push(crop.clone());
    Some(lines)
}

fn finish_report(json_mode: bool, report: &openworld_core::ScanReport) -> Result<i32, String> {
    emit(
        json_mode,
        serde_json::to_value(report).map_err(|e| e.to_string())?,
    );
    if !json_mode {
        for line in human_lines(report) {
            println!("{line}");
        }
    }
    Ok(if report.status == "complete" { 0 } else { 2 })
}

/// The lines a person reads. The order matches the result screen.
fn human_lines(report: &openworld_core::ScanReport) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(report.summary.clone());
    if report.status == "incomplete"
        && !report.message.is_empty()
        && report.message != report.summary
    {
        lines.push(report.message.clone());
    }
    if let Some(banner) = &report.coverage_banner {
        lines.push(banner.clone());
    }
    if let Some(frames) = &report.frames_note {
        lines.push(frames.clone());
    }
    if let Some(classes) = &report.class_note {
        lines.push(classes.clone());
    }
    if !report.bundle_name.is_empty() {
        lines.push(format!("Bundle: {}", report.bundle_name));
    }
    if let Some(size) = &report.detection_note {
        lines.push(size.clone());
    }
    if let Some(coverage) = &report.coverage_note {
        lines.push(coverage.clone());
    }
    if !report.perception_note.is_empty() {
        lines.push(report.perception_note.clone());
    }
    lines.extend(report.warnings.iter().cloned());
    for candidate in &report.candidates {
        lines.push(candidate.wording.clone());
        if let Some(crop) = candidate.crop.as_ref().filter(|path| !path.is_empty()) {
            lines.push("Crop".into());
            lines.push(crop.clone());
        }
        if !candidate.frame_label.is_empty() {
            lines.push(candidate.frame_label.clone());
        }
        if let Some(frame) = candidate.frame.as_ref().filter(|path| !path.is_empty()) {
            lines.push(frame.clone());
        }
        if !candidate.uncertainty.is_empty() {
            lines.push(candidate.uncertainty.clone());
        }
        let poster = if candidate.poster_title.is_empty() {
            candidate.poster_class_label.clone()
        } else if candidate.poster_class_label.is_empty() {
            candidate.poster_title.clone()
        } else {
            format!(
                "{} ({})",
                candidate.poster_title, candidate.poster_class_label
            )
        };
        if !poster.is_empty() {
            lines.push(poster);
        }
        if !candidate.fbi_url.is_empty() {
            lines.push("Open FBI page".into());
            lines.push(candidate.fbi_url.clone());
        }
    }
    if !report.inventory.is_empty() {
        lines.push("Crops from this file.".into());
        for item in &report.inventory {
            if !item.frame_label.is_empty() {
                lines.push(item.frame_label.clone());
            }
            lines.push(item.label.clone());
            if let Some(crop) = item.crop.as_ref().filter(|path| !path.is_empty()) {
                lines.push(crop.clone());
            }
        }
    }
    lines.extend(report.disclosure.iter().cloned());
    lines
}

/// The estimate a person reads, in the same order as the estimate screen.
fn file_is_old(path: &Path) -> bool {
    let mtime = std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok();
    openworld_core::media_warnings(mtime, None, SystemTime::now())
        .iter()
        .any(|line| line == openworld_core::copy::OLD_FILE)
}

fn device_lines(path: &Path) -> Vec<String> {
    let mut lines = vec![openworld_core::copy::ON_DEVICE.to_string()];
    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        if !name.is_empty() {
            lines.push(name.to_string());
        }
    }
    if file_is_old(path) {
        lines.push(openworld_core::copy::OLD_FILE.to_string());
    }
    lines.extend(
        openworld_core::copy::DISCLOSURE
            .iter()
            .map(|line| (*line).to_string()),
    );
    lines.push("Fixture posters. Real FBI photos stay off.".into());
    lines
}

fn estimate_lines(
    est: &openworld_core::estimate::Estimate,
    bundle_name: &str,
    detection: DetectionSize,
    coverage: Coverage,
    missing: bool,
    wanted: bool,
    old_file: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    if old_file {
        lines.push(openworld_core::copy::OLD_FILE.to_string());
    }
    lines.push(est.human.clone());
    lines.push(est.caveat.clone());
    if let Some(note) = &est.device_note {
        lines.push(note.clone());
    }
    if let Some(note) = &est.heat_note {
        lines.push(note.clone());
    }
    if let Some(note) = &est.battery_note {
        lines.push(note.clone());
    }
    if let Some(note) = &est.suggest_computer_text {
        lines.push(note.clone());
    }
    lines.push(format!(
        "{bundle_name}. {}. {}",
        detection.label(),
        coverage.label()
    ));
    if coverage == Coverage::Measured {
        lines.push(openworld_core::copy::BRIEF_FACE.into());
    }
    lines.push(openworld_core::copy::estimate_class_line(missing, wanted).into());
    lines
}

fn class_menu_lines() -> Vec<String> {
    vec![
        "Classes:".into(),
        "  missing — Missing".into(),
        "  wanted — Wanted".into(),
    ]
}

fn parse_class_answer(answer: &str) -> Result<bool, String> {
    match answer.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => Err(openworld_core::copy::CHOOSE_CLASS.into()),
    }
}

fn prompt_on(label: &str, json_mode: bool) -> Result<bool, String> {
    parse_class_answer(&prompt(&format!("{label}? [Y/n]:"), json_mode)?)
}

fn bundle_display_name(bundles: &Path, id: &str) -> String {
    load_bundles(bundles)
        .ok()
        .and_then(|all| {
            all.into_iter()
                .find(|item| item.id == id)
                .map(|item| item.name)
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| id.to_string())
}

fn request(
    bundles: &Path,
    input: &Path,
    bundle: &str,
    long_side: &str,
    coverage: &str,
    form_factor: &str,
    provider: &str,
    posters: PathBuf,
    out: PathBuf,
    missing: bool,
    wanted: bool,
    abort_after_frames: Option<u64>,
    frames_dir: Option<PathBuf>,
    media: Option<MediaFacts>,
) -> Result<ScanRequest, String> {
    let detection =
        DetectionSize::parse(long_side).ok_or("Detection size must be 320, 480, 640, or full.")?;
    let coverage = Coverage::parse(coverage).ok_or("Coverage must be complete or measured.")?;
    let form_factor = match form_factor {
        "phone" => FormFactor::Phone,
        "computer" => FormFactor::Computer,
        _ => return Err("Form factor must be phone or computer.".into()),
    };
    Ok(ScanRequest {
        input: input.to_path_buf(),
        bundles_dir: bundles.to_path_buf(),
        bundle_id: bundle.to_string(),
        posters_dir: posters,
        out_dir: out,
        detection,
        coverage,
        form_factor,
        execution: resolve_execution(provider),
        missing,
        wanted,
        abort_after_frames,
        frames_dir,
        media,
        now: SystemTime::now(),
    })
}

fn bundles_dir(flag: Option<PathBuf>) -> PathBuf {
    if let Some(path) = flag {
        return path;
    }
    let mut cursor = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..6 {
        if cursor.join("bundles/fast/manifest.toml").is_file() {
            return cursor.join("bundles");
        }
        if !cursor.pop() {
            break;
        }
    }
    PathBuf::from("bundles")
}

fn bundle_menu_lines(items: &[openworld_core::Bundle]) -> Vec<String> {
    let mut lines = vec![
        "Model bundle".into(),
        openworld_core::copy::BUNDLE_NOTE.into(),
    ];
    for item in items {
        lines.push(format!(
            "  {} — {}",
            item.id,
            marked_name(&item.name, item.preselected())
        ));
        lines.push(format!("    {}", item.best_for));
        lines.push(format!("    {}", item.curve_line));
    }
    lines
}

fn choose_lines() -> Vec<String> {
    vec![
        "Choose a photo or video".into(),
        openworld_core::copy::NO_CAMERA.into(),
    ]
}

fn size_menu_lines() -> Vec<String> {
    let mut lines = vec![
        "Detection size:".into(),
        openworld_core::copy::SIZE_HINT.into(),
    ];
    for (id, label) in [
        ("320", "320 px on the long side"),
        ("480", "480 px on the long side"),
        ("640", "640 px on the long side"),
        ("full", "Full resolution"),
    ] {
        lines.push(format!("  {id} — {}", marked_name(label, id == "640")));
    }
    lines
}

fn prompt(text: &str, json_mode: bool) -> Result<String, String> {
    if json_mode {
        eprint!("{text} ");
        io::stderr().flush().ok();
    } else {
        print!("{text} ");
        io::stdout().flush().ok();
    }
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    Ok(line.trim().to_string())
}

fn marked_name(name: &str, selected: bool) -> String {
    if !selected {
        return name.to_string();
    }
    if name.ends_with('.') {
        format!("{name} Selected.")
    } else {
        format!("{name}. Selected.")
    }
}

fn copy_lines(words: &serde_json::Value) -> Vec<String> {
    const ORDER: &[&str] = &[
        "on_device",
        "disclosure",
        "possible_candidate",
        "not_compared",
        "incomplete",
        "no_clearance",
        "leaving",
        "old_file",
        "timestamps_disagree",
        "brief_face",
        "vehicle_not_person",
        "below_cutoff",
        "plate_not_on_poster",
        "plate_unread",
        "plate_unpublished",
        "face_unscored",
        "fixture_markers",
        "no_class",
        "choose_class",
        "cpu_note",
        "gpu_note",
        "heat_note",
        "battery_note",
        "suggest_computer",
        "not_measured",
        "estimate_caveat",
        "no_camera",
        "size_hint",
        "bundle_note",
    ];
    let mut lines = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for key in ORDER {
        if let Some(value) = words.get(key) {
            push_copy(&mut lines, value);
            seen.insert((*key).to_string());
        }
    }
    if let Some(object) = words.as_object() {
        for (key, value) in object {
            if seen.insert(key.clone()) {
                push_copy(&mut lines, value);
            }
        }
    }
    lines
}

fn push_copy(lines: &mut Vec<String>, value: &serde_json::Value) {
    if let Some(text) = value.as_str() {
        lines.push(text.to_string());
    } else if let Some(list) = value.as_array() {
        for entry in list {
            if let Some(text) = entry.as_str() {
                lines.push(text.to_string());
            }
        }
    }
}

fn bundle_lines(rows: &[serde_json::Value]) -> Vec<String> {
    let mut lines = vec![
        "Model bundle".into(),
        openworld_core::copy::BUNDLE_NOTE.into(),
    ];
    for row in rows {
        let name = row["name"].as_str().unwrap_or("");
        lines.push(marked_name(
            name,
            row["preselected"].as_bool() == Some(true),
        ));
        lines.push(format!("  {}", row["best_for"].as_str().unwrap_or("")));
        lines.push(format!("  {}", row["curve_line"].as_str().unwrap_or("")));
    }
    lines
}

fn speak(json_mode: bool, lines: impl IntoIterator<Item = impl AsRef<str>>) {
    if json_mode {
        return;
    }
    for line in lines {
        println!("{}", line.as_ref());
    }
}

fn poster_check_lines(pack: &openworld_core::PosterPack) -> Vec<String> {
    let mut lines = Vec::new();
    if pack.perception == openworld_core::fiducial::PERCEPTION_FIDUCIAL {
        lines.push("Fixture posters. Real FBI photos stay off.".into());
    }
    let count = pack.posters.len();
    lines.push(if count == 1 {
        "1 poster.".into()
    } else {
        format!("{count} posters.")
    });
    lines
}

fn emit(json_mode: bool, value: serde_json::Value) {
    if json_mode || value.get("schema").and_then(|v| v.as_str()) == Some("openworld.measurement.v1")
    {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
        );
    }
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    fn bundles() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../bundles")
            .display()
            .to_string()
    }

    fn run_cli(json_mode: bool, args: &[&str]) -> Result<i32, String> {
        let mut argv = vec!["openworld".to_string()];
        if json_mode {
            argv.push("--json".into());
        }
        argv.push("--bundles".into());
        argv.push(bundles());
        argv.extend(args.iter().map(|item| (*item).to_string()));
        run(Cli::parse_from(argv))
    }

    #[test]
    fn the_cli_runs_the_library_commands() {
        assert_eq!(run_cli(true, &["copy"]).unwrap(), 0);
        assert_eq!(run_cli(false, &["copy"]).unwrap(), 0);
        assert_eq!(run_cli(true, &["bundles"]).unwrap(), 0);
        assert_eq!(run_cli(false, &["bundles"]).unwrap(), 0);

        let dir = tempfile::tempdir().unwrap();
        let posters = dir.path().join("posters");
        assert_eq!(
            run_cli(
                true,
                &[
                    "posters",
                    "write-fixture",
                    "--out",
                    posters.to_str().unwrap()
                ]
            )
            .unwrap(),
            0
        );
        assert_eq!(
            run_cli(
                true,
                &["posters", "check", "--posters", posters.to_str().unwrap()]
            )
            .unwrap(),
            0
        );
        assert_eq!(run_cli(true, &["posters", "update"]).unwrap(), 2);
        assert_eq!(run_cli(false, &["posters", "update"]).unwrap(), 2);
        assert_eq!(
            run_cli(
                true,
                &[
                    "posters",
                    "check",
                    "--posters",
                    "/tmp/openworld-cli-missing-pack"
                ]
            )
            .unwrap(),
            2
        );

        let scene = dir.path().join("scene.png");
        assert_eq!(
            run_cli(
                true,
                &["fixture-still", "--out", scene.to_str().unwrap(), "--scene"]
            )
            .unwrap(),
            0
        );
        let blank_path = dir.path().join("blank.png");
        assert_eq!(
            run_cli(
                false,
                &[
                    "fixture-still",
                    "--out",
                    blank_path.to_str().unwrap(),
                    "--blank"
                ]
            )
            .unwrap(),
            0
        );
        let face = dir.path().join("face.png");
        assert_eq!(
            run_cli(
                true,
                &[
                    "fixture-still",
                    "--out",
                    face.to_str().unwrap(),
                    "--id",
                    "7",
                    "--module",
                    "16",
                    "--x",
                    "16",
                    "--y",
                    "16"
                ]
            )
            .unwrap(),
            0
        );
        assert!(run_cli(
            true,
            &[
                "fixture-still",
                "--out",
                dir.path().join("both.png").to_str().unwrap(),
                "--scene",
                "--blank"
            ]
        )
        .is_err());
        assert!(run_cli(
            true,
            &[
                "fixture-still",
                "--out",
                dir.path().join("xy.png").to_str().unwrap(),
                "--x",
                "8"
            ]
        )
        .is_err());
        assert!(run_cli(
            true,
            &[
                "fixture-still",
                "--out",
                dir.path().join("below.png").to_str().unwrap(),
                "--below-cutoff",
                "--x",
                "8",
                "--y",
                "8"
            ]
        )
        .is_err());
        assert!(run_cli(
            true,
            &[
                "fixture-still",
                "--out",
                dir.path().join("mod.png").to_str().unwrap(),
                "--module",
                "0"
            ]
        )
        .is_err());
        assert!(run_cli(
            true,
            &[
                "fixture-still",
                "--out",
                dir.path().join("fit.png").to_str().unwrap(),
                "--x",
                "390",
                "--y",
                "8"
            ]
        )
        .is_err());

        assert_eq!(
            run_cli(true, &["leave", "--url", "https://www.fbi.gov/wanted"]).unwrap(),
            0
        );
        assert_eq!(
            run_cli(false, &["leave", "--url", "https://fbi.gov/"]).unwrap(),
            0
        );
        assert_eq!(
            run_cli(true, &["leave", "--url", "https://example.com"]).unwrap(),
            2
        );

        let junk = dir.path().join("notes.txt");
        std::fs::write(&junk, b"not a photo").unwrap();
        assert_eq!(
            run_cli(
                true,
                &[
                    "estimate",
                    "--input",
                    junk.to_str().unwrap(),
                    "--long-side",
                    "640"
                ]
            )
            .unwrap(),
            2
        );
        assert!(run_cli(
            true,
            &[
                "estimate",
                "--input",
                junk.to_str().unwrap(),
                "--long-side",
                "100"
            ]
        )
        .is_err());
        assert_eq!(
            run_cli(
                false,
                &[
                    "estimate",
                    "--input",
                    junk.to_str().unwrap(),
                    "--form-factor",
                    "phone",
                    "--width",
                    "1920",
                    "--height",
                    "1080",
                    "--fps",
                    "30",
                    "--frame-count",
                    "20000",
                    "--duration",
                    "600",
                    "--video",
                ],
            )
            .unwrap(),
            0
        );

        let out = dir.path().join("result");
        assert_eq!(
            run_cli(
                true,
                &[
                    "scan",
                    "--input",
                    scene.to_str().unwrap(),
                    "--posters",
                    posters.to_str().unwrap(),
                    "--out",
                    out.to_str().unwrap(),
                    "--progress",
                ],
            )
            .unwrap(),
            0
        );
        assert_eq!(
            run_cli(true, &["delete", "--out", out.to_str().unwrap()]).unwrap(),
            0
        );
        assert_eq!(
            run_cli(false, &["delete", "--out", dir.path().to_str().unwrap()]).unwrap(),
            2
        );

        let analyzed = dir.path().join("analyzed");
        assert_eq!(
            run_cli(
                true,
                &[
                    "analyze",
                    "--input",
                    blank_path.to_str().unwrap(),
                    "--bundle",
                    "fast",
                    "--long-side",
                    "640",
                    "--coverage",
                    "measured",
                    "--posters",
                    posters.to_str().unwrap(),
                    "--out",
                    analyzed.to_str().unwrap(),
                    "--form-factor",
                    "phone",
                    "--yes",
                ],
            )
            .unwrap(),
            0
        );
        let demo_dir = dir.path().join("demo");
        assert_eq!(
            run_cli(true, &["demo", "--out", demo_dir.to_str().unwrap()]).unwrap(),
            0
        );
        assert_eq!(
            run_cli(
                false,
                &["demo", "--out", dir.path().join("demo2").to_str().unwrap()]
            )
            .unwrap(),
            0
        );

        assert_eq!(run_cli(true, &["measure"]).unwrap(), 0);

        let listed = run(Cli::parse_from(["openworld", "copy"])).unwrap();
        assert_eq!(listed, 0);
    }

    #[test]
    fn the_human_report_lists_each_candidate_card() {
        let dir = tempfile::tempdir().unwrap();
        let demo_dir = dir.path().join("demo");
        assert_eq!(demo(Path::new(&bundles()), &demo_dir, true).unwrap(), 0);
        let bytes = std::fs::read(demo_dir.join("result/result.json")).unwrap();
        let mut report: openworld_core::ScanReport = serde_json::from_slice(&bytes).unwrap();
        report.warnings.push("The file timestamps disagree.".into());
        let lines = human_lines(&report);
        let at = |text: &str| {
            lines
                .iter()
                .position(|line| line == text)
                .unwrap_or_else(|| panic!("missing {text} in {lines:?}"))
        };
        let summary = at("Possible candidate. Not an identification.");
        let warning = at("The file timestamps disagree.");
        let face = at("Fixture subject A (Missing)");
        let plate = at("Fixture vehicle C (Wanted)");
        let crops = at("Crops from this file.");
        let disclosure = at("Nothing is uploaded.");
        assert!(summary < warning && warning < face.min(plate));
        assert!(face.max(plate) < crops && crops < disclosure);
        for poster in ["Fixture subject A (Missing)", "Fixture vehicle C (Wanted)"] {
            let card = at(poster);
            assert_eq!(lines[card + 1], "Open FBI page");
            assert!(
                lines[card + 2].starts_with("https://www.fbi.gov"),
                "{}",
                lines[card + 2]
            );
            assert!(
                !lines[card - 1].contains("Possible candidate. Not an identification."),
                "{}",
                lines[card - 1]
            );
            assert!(lines[..card]
                .iter()
                .rev()
                .take(5)
                .any(|line| line == "Frame 1."));
        }
        assert!(lines.iter().any(|line| line == "Bundle: Fast"));
        assert!(lines.iter().any(|line| line == "1 frame analyzed."));
        assert!(lines.iter().any(|line| line == "Every decoded frame."));
        assert!(lines.iter().all(|line| {
            !line.starts_with("face  ")
                && !line.starts_with("plate  ")
                && !line.starts_with("vehicle  ")
        }));

        report.status = "incomplete".into();
        report.summary = "Incomplete.".into();
        report.message = "The file was not fully decoded.".into();
        report.candidates.clear();
        report.warnings.clear();
        let unfinished = human_lines(&report);
        assert_eq!(unfinished[0], "Incomplete.");
        assert_eq!(unfinished[1], "The file was not fully decoded.");

        let refused =
            openworld_core::scan::refused("unreadable", "The file could not be read. Refusing.");
        let refused_lines = human_lines(&refused);
        assert_eq!(refused_lines[0], "The file could not be read. Refusing.");
        assert!(refused_lines
            .iter()
            .all(|line| !line.starts_with("Bundle:")));
        assert!(refused_lines
            .iter()
            .all(|line| line != "No candidate is not a clearance."));
    }

    #[test]
    fn the_estimate_a_person_reads_matches_the_screen() {
        let measured =
            openworld_core::estimate::estimate(&openworld_core::estimate::EstimateInput {
                frames: 27_000,
                fps: 30.0,
                duration_sec: 900.0,
                original_long_side: 1920,
                detection: DetectionSize::Px(640),
                coverage: Coverage::Measured,
                bundle_factor: 1.0,
                execution: openworld_core::Execution::Cpu,
                form_factor: FormFactor::Phone,
            });
        let lines = estimate_lines(
            &measured,
            "Fast",
            DetectionSize::Px(640),
            Coverage::Measured,
            true,
            true,
            false,
        );
        assert_eq!(lines[0], measured.human);
        assert_eq!(
            lines[1],
            "This is a planning estimate, not a thermal measurement."
        );
        let choice = lines
            .iter()
            .position(|line| {
                line == "Fast. 640 px on the long side. 5 frames a second, plus the tracker."
            })
            .expect("choice");
        let brief = lines
            .iter()
            .position(|line| line == "A brief face can be missed.")
            .expect("brief");
        assert!(lines[..choice].iter().any(|line| line.contains("CPU")));
        assert!(lines[..choice]
            .iter()
            .any(|line| line.contains("This phone may get hot.")));
        assert!(lines[..choice]
            .iter()
            .any(|line| line.contains("A long scan uses a lot of battery.")));
        assert!(lines[..choice]
            .iter()
            .any(|line| line.contains("A computer will finish this sooner.")));
        assert!(choice < brief);
        assert_eq!(
            lines.last().map(String::as_str),
            Some("Missing and wanted.")
        );
        let aged = estimate_lines(
            &measured,
            "Fast",
            DetectionSize::Px(640),
            Coverage::Measured,
            true,
            true,
            true,
        );
        assert_eq!(aged[0], "This file is older than about 30 days.");
        assert_eq!(aged[1], measured.human);
        assert!(aged
            .iter()
            .all(|line| line != "The file timestamps disagree."));
        assert_eq!(aged.last().map(String::as_str), Some("Missing and wanted."));
        let missing_only = estimate_lines(
            &measured,
            "Fast",
            DetectionSize::Px(640),
            Coverage::Measured,
            true,
            false,
            false,
        );
        let brief = missing_only
            .iter()
            .position(|line| line == "A brief face can be missed.")
            .expect("brief");
        assert!(brief + 1 == missing_only.len() - 1);
        assert_eq!(missing_only.last().map(String::as_str), Some("Missing."));
        assert!(missing_only
            .iter()
            .all(|line| line != "Missing and wanted."));
        let neither = estimate_lines(
            &measured,
            "Fast",
            DetectionSize::Px(640),
            Coverage::Measured,
            false,
            false,
            false,
        );
        assert_eq!(
            neither.last().map(String::as_str),
            Some("Choose missing, wanted, or both.")
        );

        let still = estimate_lines(
            &openworld_core::estimate::estimate(&openworld_core::estimate::EstimateInput {
                frames: 1,
                fps: 0.0,
                duration_sec: 0.0,
                original_long_side: 640,
                detection: DetectionSize::Full,
                coverage: Coverage::Complete,
                bundle_factor: 1.0,
                execution: openworld_core::Execution::Cpu,
                form_factor: FormFactor::Computer,
            }),
            "Fast",
            DetectionSize::Full,
            Coverage::Complete,
            false,
            true,
            false,
        );
        assert!(still
            .iter()
            .all(|line| line != "A brief face can be missed."));
        assert!(still.iter().all(|line| !line.contains("hot")));
        assert_eq!(
            still.iter().rev().nth(1).map(String::as_str),
            Some("Fast. Full resolution. Every decoded frame.")
        );
        assert_eq!(still.last().map(String::as_str), Some("Wanted."));
        assert_eq!(
            class_menu_lines(),
            vec![
                "Classes:".to_string(),
                "  missing — Missing".to_string(),
                "  wanted — Wanted".to_string(),
            ]
        );
        assert_eq!(parse_class_answer("").unwrap(), true);
        assert_eq!(parse_class_answer("n").unwrap(), false);
        assert_eq!(
            parse_class_answer("maybe").unwrap_err(),
            "Choose missing, wanted, or both."
        );
    }

    #[test]
    fn a_live_crop_uses_the_words_on_the_screen() {
        let face = Progress {
            frame_index: 0,
            frame_label: "Frame 1.".into(),
            label: "Possible candidate. Not an identification.".into(),
            kind: "face".into(),
            crop: Some("crops/face.png".into()),
        };
        assert_eq!(
            live_crop_lines(&face).unwrap(),
            vec![
                "Frame 1.".to_string(),
                "Possible candidate. Not an identification.".to_string(),
                "crops/face.png".to_string(),
            ]
        );
        let plate = Progress {
            frame_index: 1,
            frame_label: "Frame 2.".into(),
            label: "No poster publishes a plate.".into(),
            kind: "plate".into(),
            crop: Some("crops/plate.png".into()),
        };
        assert_eq!(live_crop_lines(&plate).unwrap()[0], "Frame 2.");
        assert!(live_crop_lines(&Progress {
            frame_index: 0,
            frame_label: "Frame 1.".into(),
            label: "Skipped.".into(),
            kind: "other".into(),
            crop: Some("crops/x.png".into()),
        })
        .is_none());
        assert!(live_crop_lines(&Progress {
            frame_index: 0,
            frame_label: "Frame 1.".into(),
            label: String::new(),
            kind: "vehicle".into(),
            crop: Some("crops/x.png".into()),
        })
        .is_none());
        assert!(live_crop_lines(&Progress {
            frame_index: 0,
            frame_label: "Frame 1.".into(),
            label: "A vehicle is not a person.".into(),
            kind: "vehicle".into(),
            crop: None,
        })
        .is_none());
    }

    #[test]
    fn delete_leave_and_poster_check_say_what_the_screen_says() {
        let dir = tempfile::tempdir().unwrap();
        let err = delete_output(dir.path()).unwrap_err();
        assert_eq!(
            err,
            "Refusing to delete a directory that is not an OpenWorld result."
        );
        let blocked = leave_prompt("https://example.com").unwrap_err();
        assert_eq!(blocked, "OpenWorld only opens an FBI page.");
        let pack = write_fixture_pack(dir.path(), SystemTime::now()).unwrap();
        assert_eq!(
            poster_check_lines(&pack),
            vec![
                "Fixture posters. Real FBI photos stay off.".to_string(),
                "3 posters.".to_string(),
            ]
        );
        assert_eq!(
            run_cli(
                false,
                &[
                    "posters",
                    "check",
                    "--posters",
                    dir.path().join("missing").to_str().unwrap()
                ]
            )
            .unwrap(),
            2
        );
    }

    #[test]
    fn copy_and_bundles_use_the_words_on_the_screen() {
        let lines = copy_lines(&product_copy());
        for phrase in [
            "Possible candidate. Not an identification.",
            "Not compared.",
            "Incomplete.",
            "No candidate is not a clearance.",
            "You are leaving OpenWorld.",
            "A brief face can be missed.",
            "This file stays on this device.",
            "Nothing is uploaded.",
            "Not measured yet.",
            "Choose missing, wanted, or both.",
            "Import a file you already have. There is no camera.",
            "Scores are not comparable across bundles. Results name the bundle you pick.",
        ] {
            assert!(lines.iter().any(|line| line == phrase), "{phrase} missing");
        }
        assert!(lines.iter().any(|line| {
            line.contains("A face under 64 px on that image is left out.")
                && line.contains("the label is \"Not compared.\"")
                && !line.contains("about 112")
        }));
        let device = lines
            .iter()
            .position(|line| line == "This file stays on this device.")
            .unwrap();
        let uploaded = lines
            .iter()
            .position(|line| line == "Nothing is uploaded.")
            .unwrap();
        let possible = lines
            .iter()
            .position(|line| line == "Possible candidate. Not an identification.")
            .unwrap();
        let caveat = lines
            .iter()
            .position(|line| line == "This is a planning estimate, not a thermal measurement.")
            .unwrap();
        assert!(device < uploaded && uploaded < possible && possible < caveat);
        let all = load_bundles(Path::new(&bundles())).unwrap();
        let rows: Vec<_> = all.iter().map(openworld_core::bundle::row_json).collect();
        let listed = bundle_lines(&rows);
        assert_eq!(listed[0], "Model bundle");
        assert_eq!(
            listed[1],
            "Scores are not comparable across bundles. Results name the bundle you pick."
        );
        assert!(listed.iter().any(|line| line == "Fast. Selected."));
        assert!(listed.iter().any(|line| line == "Accurate"));
        assert!(listed.iter().all(|line| line != "Accurate. Selected."));
        assert!(listed.iter().any(|line| line.contains("Not measured yet.")));
        let menu = bundle_menu_lines(&all);
        assert_eq!(menu[0], "Model bundle");
        assert_eq!(menu[1], listed[1]);
        assert!(menu.iter().any(|line| line == "  fast — Fast. Selected."));
        assert_eq!(marked_name("Fast", true), "Fast. Selected.");
        assert_eq!(marked_name("Accurate", false), "Accurate");
        assert_eq!(
            marked_name("Complete. Every decoded frame.", true),
            "Complete. Every decoded frame. Selected."
        );
        assert_eq!(
            marked_name("640 px on the long side", true),
            "640 px on the long side. Selected."
        );
        assert_eq!(
            choose_lines(),
            vec![
                "Choose a photo or video".to_string(),
                "Import a file you already have. There is no camera.".to_string(),
            ]
        );
        let size = size_menu_lines();
        assert_eq!(size[0], "Detection size:");
        assert!(size[1].contains("A face under 64 px on that image is left out."));
        assert!(size[1].contains("the label is \"Not compared.\""));
        assert!(!size[1].contains("about 112"));
        let selected = size
            .iter()
            .position(|line| line == "  640 — 640 px on the long side. Selected.")
            .expect("selected size");
        assert!(1 < selected);
        assert!(size.iter().any(|line| line == "  full — Full resolution"));
    }
}
