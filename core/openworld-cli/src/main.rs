// SPDX-License-Identifier: Apache-2.0

use clap::{Parser, Subcommand};
use openworld_core::copy::product_copy;
use openworld_core::estimate::{Coverage, DetectionSize, FormFactor};
use openworld_core::hardware::execution_from_provider;
use openworld_core::hardware::loaded_execution;
use openworld_core::posters::write_fixture_pack;
use openworld_core::scan::{delete_output, estimate_for, leave_prompt, scan_path, Progress, ScanRequest};
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
        #[arg(long)]
        yes: bool,
    },
    /// Synthetic fixture scan. No photograph of a person.
    Demo {
        #[arg(long)]
        out: PathBuf,
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
            emit(cli.json, product_copy());
            Ok(0)
        }
        Cmd::Bundles => {
            let all = load_bundles(&bundles).map_err(|e| e.to_string())?;
            let rows: Vec<_> = all.iter().map(openworld_core::bundle::row_json).collect();
            emit(cli.json, json!({ "bundles": rows }));
            if !cli.json {
                for row in &rows {
                    println!(
                        "{}\n  {}\n  {}",
                        row["name"].as_str().unwrap_or(""),
                        row["best_for"].as_str().unwrap_or(""),
                        row["curve_line"].as_str().unwrap_or("")
                    );
                }
            }
            Ok(0)
        }
        Cmd::Estimate { input, bundle, long_side, coverage, form_factor, provider } => {
            let req = request(&bundles, &input, &bundle, &long_side, &coverage, &form_factor, &provider, PathBuf::from("."), PathBuf::from("."), true, true, None)?;
            match estimate_for(&req) {
                Ok(est) => {
                    emit(cli.json, serde_json::to_value(&est).map_err(|e| e.to_string())?);
                    if !cli.json {
                        println!("{}", est.human);
                        println!("{}", est.caveat);
                        if let Some(note) = &est.device_note {
                            println!("{note}");
                        }
                        if let Some(note) = &est.heat_note {
                            println!("{note}");
                        }
                        if let Some(note) = &est.battery_note {
                            println!("{note}");
                        }
                        if let Some(note) = &est.suggest_computer_text {
                            println!("{note}");
                        }
                    }
                    Ok(0)
                }
                Err(report) => {
                    emit(cli.json, serde_json::to_value(&report).map_err(|e| e.to_string())?);
                    Ok(2)
                }
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
            )?;
            let mut progress_fn = |event: Progress| {
                if progress {
                    if let Ok(line) = serde_json::to_string(&event) {
                        eprintln!("{line}");
                    }
                }
            };
            let report = scan_path(&req, &mut progress_fn);
            finish_report(cli.json, &report)
        }
        Cmd::Analyze {
            input,
            bundle,
            long_side,
            coverage,
            posters,
            out,
            form_factor,
            yes,
        } => analyze(cli.json, &bundles, input, bundle, long_side, coverage, posters, out, &form_factor, yes),
        Cmd::Demo { out } => demo(&bundles, &out, cli.json),
        Cmd::Delete { out } => match delete_output(&out) {
            Ok(()) => {
                emit(cli.json, json!({ "deleted": true }));
                Ok(0)
            }
            Err(err) => {
                emit(cli.json, json!({ "deleted": false, "message": err }));
                Ok(2)
            }
        },
        Cmd::Leave { url } => match leave_prompt(&url) {
            Ok(prompt) => {
                emit(cli.json, serde_json::to_value(&prompt).map_err(|e| e.to_string())?);
                if !cli.json {
                    println!("{}", prompt.message);
                    println!("{}", prompt.url);
                }
                Ok(0)
            }
            Err(err) => {
                emit(cli.json, json!({ "message": err }));
                Ok(2)
            }
        },
        Cmd::Measure => {
            let all = load_bundles(&bundles).map_err(|e| e.to_string())?;
            let fast = all.into_iter().find(|b| b.id == "fast").ok_or("Fast bundle is missing.")?;
            let measurement = measure_fast(&fast);
            emit(true, serde_json::to_value(&measurement).map_err(|e| e.to_string())?);
            Ok(0)
        }
        Cmd::Posters { action } => match action {
            PosterCmd::Check { posters } => {
                match openworld_core::load_pack(&posters, SystemTime::now()) {
                    Ok(pack) => {
                        emit(cli.json, json!({
                            "id": pack.id,
                            "posters": pack.posters.len(),
                            "perception": pack.perception,
                            "expires_at": pack.expires_at,
                        }));
                        Ok(0)
                    }
                    Err(err) => {
                        emit(cli.json, json!({ "status": "refused", "message": err.to_string() }));
                        Ok(2)
                    }
                }
            }
            PosterCmd::WriteFixture { out } => {
                let pack = write_fixture_pack(&out, SystemTime::now()).map_err(|e| e.to_string())?;
                emit(
                    cli.json,
                    json!({
                        "id": pack.id,
                        "posters": pack.posters.len(),
                        "perception": pack.perception,
                        "note": "Fixture posters. Real FBI photos stay off.",
                    }),
                );
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
                        emit(cli.json, json!({ "status": "refused", "message": message, "detail": err.to_string() }));
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
    yes: bool,
) -> Result<i32, String> {
    println!("{}", openworld_core::copy::ON_DEVICE);
    for line in openworld_core::copy::DISCLOSURE {
        println!("{line}");
    }
    let input = match input {
        Some(path) => path,
        None => PathBuf::from(prompt("Photo or video path:")?),
    };
    let all = load_bundles(bundles).map_err(|e| e.to_string())?;
    if !json_mode {
        println!("Bundles:");
        for item in &all {
            let mark = if item.preselected() { " (selected)" } else { "" };
            println!("  {} — {} — {}{mark}", item.id, item.best_for, item.curve_line);
        }
    }
    let bundle = match bundle {
        Some(id) => id,
        None => {
            let entered = prompt("Bundle [fast]:")?;
            if entered.is_empty() { "fast".into() } else { entered }
        }
    };
    let long_side = match long_side {
        Some(value) => value,
        None => {
            let entered = prompt("Detection long side (320, 480, 640, full) [640]:")?;
            if entered.is_empty() { "640".into() } else { entered }
        }
    };
    let coverage = match coverage {
        Some(value) => value,
        None => {
            let entered = prompt("Coverage (complete, measured) [complete]:")?;
            if entered.is_empty() { "complete".into() } else { entered }
        }
    };
    let posters = match posters {
        Some(path) => path,
        None => PathBuf::from(prompt("Poster pack directory:")?),
    };
    let out = match out {
        Some(path) => path,
        None => PathBuf::from(prompt("Result directory:")?),
    };
    let req = request(bundles, &input, &bundle, &long_side, &coverage, form_factor, "cpu", posters, out, true, true, None)?;
    match estimate_for(&req) {
        Ok(est) => {
            println!("{}", est.human);
            println!("{}", est.caveat);
            if let Some(note) = &est.device_note {
                println!("{note}");
            }
            if let Some(note) = &est.heat_note {
                println!("{note}");
            }
            if let Some(note) = &est.battery_note {
                println!("{note}");
            }
            if let Some(note) = &est.suggest_computer_text {
                println!("{note}");
            }
            if req.coverage == Coverage::Measured {
                println!("{}", openworld_core::copy::BRIEF_FACE);
            }
        }
        Err(report) => return finish_report(json_mode, &report),
    }
    if !yes {
        let answer = prompt("Analyze? [y/N]:")?;
        if !matches!(answer.as_str(), "y" | "Y" | "yes") {
            println!("Not started.");
            return Ok(0);
        }
    }
    let report = scan_path(&req, &mut |_| {});
    finish_report(json_mode, &report)
}

fn demo(bundles: &Path, out: &Path, json_mode: bool) -> Result<i32, String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let posters = out.join("posters");
    let pack = write_fixture_pack(&posters, SystemTime::now()).map_err(|e| e.to_string())?;
    let fast = load_bundles(bundles).map_err(|e| e.to_string())?.into_iter().find(|b| b.id == "fast").ok_or("Fast bundle is missing.")?;
    let scene = demo_scene(fast.threshold);
    let input = out.join("input.png");
    scene.image.save(&input).map_err(|e| e.to_string())?;
    let result = out.join("result");
    let req = request(bundles, &input, "fast", "640", "complete", "computer", "cpu", posters, result, true, true, None)?;
    let report = scan_path(&req, &mut |_| {});
    let _ = pack;
    finish_report(json_mode, &report)
}

fn finish_report(json_mode: bool, report: &openworld_core::ScanReport) -> Result<i32, String> {
    emit(json_mode, serde_json::to_value(report).map_err(|e| e.to_string())?);
    if !json_mode {
        println!("{}", report.summary);
        if let Some(banner) = &report.coverage_banner {
            println!("{banner}");
        }
        println!("Bundle: {}", report.bundle_name);
        for line in &report.disclosure {
            println!("{line}");
        }
        for item in &report.inventory {
            println!("{}  {}  {}", item.kind, item.label, item.crop.as_deref().unwrap_or(""));
        }
    }
    Ok(if report.status == "complete" { 0 } else { 2 })
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
) -> Result<ScanRequest, String> {
    let detection = DetectionSize::parse(long_side).ok_or("Detection size must be 320, 480, 640, or full.")?;
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
        execution: if provider == "cpu" { loaded_execution() } else { execution_from_provider(provider) },
        missing,
        wanted,
        abort_after_frames,
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

fn prompt(text: &str) -> Result<String, String> {
    print!("{text} ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(line.trim().to_string())
}

fn emit(json_mode: bool, value: serde_json::Value) {
    if json_mode || value.get("schema").and_then(|v| v.as_str()) == Some("openworld.measurement.v1") {
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into()));
    }
}

