// SPDX-License-Identifier: Apache-2.0

//! The argument list the phone shells already build, run inside the library.
//! A store build calls this through `ow_command` instead of starting a program.

use crate::estimate::{Coverage, DetectionSize, FormFactor};
use crate::hardware::resolve_execution;
use crate::posters::write_fixture_pack;
use crate::scan::{estimate_for, scan_path, MediaFacts, ScanRequest};
use crate::update::{self, Transport};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub fn invoke_json(text: &str) -> String {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return refused("The scan request was not JSON. Refusing."),
    };
    let Some(argv) = value.get("argv").and_then(|v| v.as_array()) else {
        return refused("The scan request was not JSON. Refusing.");
    };
    let mut args = Vec::with_capacity(argv.len());
    for item in argv {
        let Some(text) = item.as_str() else {
            return refused("The scan request was not JSON. Refusing.");
        };
        args.push(text.to_string());
    }
    invoke_argv(&args)
}

pub fn invoke_argv(args: &[String]) -> String {
    match dispatch(args) {
        Ok(value) => value.to_string(),
        Err(message) => refused(&message),
    }
}

fn refused(message: &str) -> String {
    json!({ "status": "refused", "summary": message, "message": message }).to_string()
}

fn dispatch(args: &[String]) -> Result<Value, String> {
    let parsed = parse_args(args)?;
    let bundles = bundles_dir(parsed.bundles.clone());
    match parsed.cmd.as_str() {
        "bundles" => {
            let all = crate::load_bundles(&bundles).map_err(|err| format!("{err}. Refusing."))?;
            let rows: Vec<_> = all.iter().map(crate::bundle::row_json).collect();
            Ok(json!({ "bundles": rows }))
        }
        "estimate" => {
            let req = scan_request(&bundles, &parsed)?;
            match estimate_for(&req) {
                Ok(estimate) => serde_json::to_value(&estimate).map_err(|err| err.to_string()),
                Err(report) => serde_json::to_value(&report).map_err(|err| err.to_string()),
            }
        }
        "scan" => {
            let req = scan_request(&bundles, &parsed)?;
            let report = scan_path(&req, &mut |_| {});
            serde_json::to_value(&report).map_err(|err| err.to_string())
        }
        "posters" => posters(&bundles, &parsed),
        "leave" => {
            let url = parsed.flag("url").ok_or("OpenWorld only opens an FBI page.")?;
            match crate::scan::leave_prompt(url) {
                Ok(prompt) => serde_json::to_value(&prompt).map_err(|err| err.to_string()),
                Err(err) => Err(err),
            }
        }
        "delete" => {
            let out = parsed
                .flag("out")
                .ok_or("A result directory is required. Refusing.")?;
            match crate::scan::delete_output(Path::new(out)) {
                Ok(()) => Ok(json!({ "deleted": true })),
                Err(err) => Ok(json!({ "deleted": false, "message": err })),
            }
        }
        "" => Err("The scan request was empty. Refusing.".into()),
        other => Err(format!(
            "The command {other} is not available from the library. Refusing."
        )),
    }
}

fn posters(bundles: &Path, parsed: &Parsed) -> Result<Value, String> {
    match parsed.action.as_deref() {
        Some("write-fixture") => {
            let out = parsed
                .flag("out")
                .ok_or("A poster pack directory is required. Refusing.")?;
            let pack = write_fixture_pack(Path::new(out), SystemTime::now())
                .map_err(|err| format!("{err}. Refusing."))?;
            Ok(json!({
                "id": pack.id,
                "posters": pack.posters.len(),
                "perception": pack.perception,
                "note": "Fixture posters. Real FBI photos stay off.",
            }))
        }
        Some("update") => {
            let allowed = crate::load_bundles(bundles)
                .ok()
                .and_then(|all| all.into_iter().find(|bundle| bundle.id == "fast"))
                .map(|bundle| bundle.real_posters_allowed)
                .unwrap_or(false);
            match update::update_posters(allowed, &mut RefuseTransport) {
                Ok(pack) => {
                    Ok(json!({ "posters": pack.posters.len(), "body_sha256": pack.body_sha256 }))
                }
                Err(_) => Ok(json!({
                    "status": "refused",
                    "summary": "Real FBI photos stay off. Fast does not have a curve that allows them.",
                    "message": "Real FBI photos stay off. Fast does not have a curve that allows them.",
                })),
            }
        }
        Some("check") => {
            let path = parsed
                .flag("posters")
                .ok_or("A poster pack directory is required. Refusing.")?;
            match crate::load_pack(Path::new(path), SystemTime::now()) {
                Ok(pack) => Ok(json!({
                    "id": pack.id,
                    "posters": pack.posters.len(),
                    "perception": pack.perception,
                    "expires_at": pack.expires_at,
                })),
                Err(err) => Ok(json!({ "status": "refused", "message": err.to_string() })),
            }
        }
        _ => Err("The poster command is not recognized. Refusing.".into()),
    }
}

struct RefuseTransport;
impl Transport for RefuseTransport {
    fn get(&mut self, _url: &str) -> Result<Vec<u8>, String> {
        Err("real FBI photos stay off".into())
    }
}

struct Parsed {
    bundles: Option<PathBuf>,
    cmd: String,
    action: Option<String>,
    flags: HashMap<String, String>,
    bools: HashSet<String>,
}

impl Parsed {
    fn flag(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(String::as_str)
    }
}

fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut bundles = None;
    let mut cmd = String::new();
    let mut rest = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--json" || arg == "--progress" {
            index += 1;
            continue;
        }
        if arg == "--bundles" {
            index += 1;
            let path = args
                .get(index)
                .ok_or("A bundle catalog path is required. Refusing.")?;
            bundles = Some(PathBuf::from(path));
            index += 1;
            continue;
        }
        if cmd.is_empty() && !arg.starts_with('-') {
            cmd = arg.clone();
            index += 1;
            continue;
        }
        rest.push(arg.clone());
        index += 1;
    }
    let (action, rest) = if rest.first().is_some_and(|item| !item.starts_with('-')) {
        (rest.first().cloned(), rest[1..].to_vec())
    } else {
        (None, rest)
    };
    let (flags, bools) = flags_from(&rest)?;
    Ok(Parsed {
        bundles,
        cmd,
        action,
        flags,
        bools,
    })
}

fn flags_from(rest: &[String]) -> Result<(HashMap<String, String>, HashSet<String>), String> {
    let mut flags = HashMap::new();
    let mut bools = HashSet::new();
    let mut index = 0;
    while index < rest.len() {
        let arg = &rest[index];
        if arg == "--video" || arg == "--no-missing" || arg == "--no-wanted" {
            bools.insert(arg.trim_start_matches("--").replace('-', "_"));
            index += 1;
            continue;
        }
        let Some(name) = arg.strip_prefix("--") else {
            return Err(format!("Unexpected argument {arg}. Refusing."));
        };
        index += 1;
        let value = rest
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Missing value for --{name}. Refusing."))?;
        flags.insert(name.replace('-', "_"), value);
        index += 1;
    }
    Ok((flags, bools))
}

fn scan_request(bundles: &Path, parsed: &Parsed) -> Result<ScanRequest, String> {
    let input = PathBuf::from(parsed.flag("input").unwrap_or(""));
    let bundle = parsed.flag("bundle").unwrap_or("fast").to_string();
    let long_side = parsed.flag("long_side").unwrap_or("640");
    let coverage = parsed.flag("coverage").unwrap_or("complete");
    let form = parsed.flag("form_factor").unwrap_or("computer");
    let provider = parsed.flag("provider").unwrap_or("cpu");
    let detection =
        DetectionSize::parse(long_side).ok_or("Detection size must be 320, 480, 640, or full.")?;
    let coverage = Coverage::parse(coverage).ok_or("Coverage must be complete or measured.")?;
    let form_factor = match form {
        "phone" => FormFactor::Phone,
        "computer" => FormFactor::Computer,
        _ => return Err("Form factor must be phone or computer.".into()),
    };
    let posters = PathBuf::from(parsed.flag("posters").unwrap_or("."));
    let out = PathBuf::from(parsed.flag("out").unwrap_or("."));
    let frames_dir = parsed.flag("frames").map(PathBuf::from);
    let abort_after_frames = match parsed.flag("abort_after_frames") {
        Some(text) => Some(
            text.parse::<u64>()
                .map_err(|_| "abort-after-frames must be a number. Refusing.".to_string())?,
        ),
        None => None,
    };
    Ok(ScanRequest {
        input,
        bundles_dir: bundles.to_path_buf(),
        bundle_id: bundle,
        posters_dir: posters,
        out_dir: out,
        detection,
        coverage,
        form_factor,
        execution: resolve_execution(provider),
        missing: !parsed.bools.contains("no_missing"),
        wanted: !parsed.bools.contains("no_wanted"),
        abort_after_frames,
        frames_dir,
        media: media_from(parsed)?,
        now: SystemTime::now(),
    })
}

fn media_from(parsed: &Parsed) -> Result<Option<MediaFacts>, String> {
    let touched = parsed.bools.contains("video")
        || [
            "width",
            "height",
            "fps",
            "frame_count",
            "duration",
            "container_unix",
        ]
        .iter()
        .any(|key| parsed.flags.contains_key(*key));
    if !touched {
        return Ok(None);
    }
    let width = parsed
        .flag("width")
        .ok_or("A platform decode needs a width.")?
        .parse::<u32>()
        .map_err(|_| "A platform decode needs a width.")?;
    let height = parsed
        .flag("height")
        .ok_or("A platform decode needs a height.")?
        .parse::<u32>()
        .map_err(|_| "A platform decode needs a height.")?;
    let frames = parsed
        .flag("frame_count")
        .ok_or("A platform decode needs a frame count.")?
        .parse::<u64>()
        .map_err(|_| "A platform decode needs a frame count.")?;
    if width == 0 || height == 0 || frames == 0 {
        return Err("Bad codec or unreadable file. Refusing.".into());
    }
    let fps = match parsed.flag("fps") {
        Some(text) => text
            .parse::<f64>()
            .map_err(|_| "A platform decode needs a frame rate.".to_string())?,
        None => 0.0,
    };
    let duration_sec = match parsed.flag("duration") {
        Some(text) => text
            .parse::<f64>()
            .map_err(|_| "A platform decode needs a duration.".to_string())?,
        None => 0.0,
    };
    let container_unix = match parsed.flag("container_unix") {
        Some(text) => Some(
            text.parse::<u64>()
                .map_err(|_| "container-unix must be a number. Refusing.".to_string())?,
        ),
        None => None,
    };
    Ok(Some(MediaFacts {
        width,
        height,
        fps,
        frames,
        duration_sec,
        video: parsed.bools.contains("video"),
        container_unix,
    }))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_bundles() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../bundles")
            .display()
            .to_string()
    }

    #[test]
    fn the_library_lists_fast_and_refuses_a_poster_update() {
        let bundles = repo_bundles();
        let listed: Value = serde_json::from_str(&invoke_argv(&[
            "--json".into(),
            "--bundles".into(),
            bundles.clone(),
            "bundles".into(),
        ]))
        .unwrap();
        let ids: Vec<_> = listed["bundles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap().to_string())
            .collect();
        assert!(ids.contains(&"fast".to_string()));
        assert!(ids.contains(&"accurate".to_string()));

        let update: Value = serde_json::from_str(&invoke_argv(&[
            "--json".into(),
            "--bundles".into(),
            bundles,
            "posters".into(),
            "update".into(),
        ]))
        .unwrap();
        assert_eq!(update["status"], "refused");
        assert_eq!(
            update["message"],
            "Real FBI photos stay off. Fast does not have a curve that allows them."
        );
    }

    #[test]
    fn the_library_opens_an_fbi_page_and_refuses_a_lookalike() {
        let allowed: Value = serde_json::from_str(&invoke_argv(&[
            "--json".into(),
            "leave".into(),
            "--url".into(),
            "https://www.fbi.gov/wanted".into(),
        ]))
        .unwrap();
        assert_eq!(allowed["message"], "You are leaving OpenWorld.");
        assert_eq!(allowed["url"], "https://www.fbi.gov/wanted");

        let exact: Value = serde_json::from_str(&invoke_argv(&[
            "--json".into(),
            "leave".into(),
            "--url".into(),
            "https://fbi.gov/wanted".into(),
        ]))
        .unwrap();
        assert_eq!(exact["url"], "https://fbi.gov/wanted");

        for blocked in [
            "https://www.fbi.gov.evil.com/wanted",
            "http://www.fbi.gov/wanted",
            "https://www.fbi.gov@evil.com/wanted",
            "https://evil.com/?next=https://www.fbi.gov/wanted",
        ] {
            let lookalike: Value = serde_json::from_str(&invoke_argv(&[
                "leave".into(),
                "--url".into(),
                blocked.into(),
            ]))
            .unwrap();
            assert_eq!(lookalike["status"], "refused", "{blocked}");
            assert_eq!(lookalike["message"], "OpenWorld only opens an FBI page.");
            assert!(lookalike.get("url").is_none(), "{blocked}");
        }
    }

    #[test]
    fn the_library_deletes_a_result_and_refuses_anything_else() {
        let missing: Value = serde_json::from_str(&invoke_argv(&["delete".into()])).unwrap();
        assert_eq!(missing["status"], "refused");
        assert_eq!(missing["message"], "A result directory is required. Refusing.");

        let dir = std::env::temp_dir().join(format!("ow-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("result.json"), b"{}").unwrap();
        std::fs::write(dir.join("crop.png"), b"crop").unwrap();
        let path = dir.display().to_string();
        let deleted: Value = serde_json::from_str(&invoke_argv(&[
            "--json".into(),
            "delete".into(),
            "--out".into(),
            path.clone(),
        ]))
        .unwrap();
        assert_eq!(deleted["deleted"], true);
        assert!(!dir.exists());

        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), b"keep").unwrap();
        let refused: Value = serde_json::from_str(&invoke_argv(&[
            "delete".into(),
            "--out".into(),
            path,
        ]))
        .unwrap();
        assert_eq!(refused["deleted"], false);
        assert_eq!(
            refused["message"],
            "Refusing to delete a directory that is not an OpenWorld result."
        );
        assert!(dir.join("notes.txt").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_named_gpu_still_warns_from_the_loaded_cpu() {
        let bundles = repo_bundles();
        let text = invoke_argv(&[
            "--json".into(),
            "--bundles".into(),
            bundles,
            "estimate".into(),
            "--input".into(),
            "unused.png".into(),
            "--provider".into(),
            "cuda".into(),
            "--form-factor".into(),
            "phone".into(),
            "--width".into(),
            "1280".into(),
            "--height".into(),
            "720".into(),
            "--fps".into(),
            "30".into(),
            "--frame-count".into(),
            "30".into(),
            "--duration".into(),
            "1".into(),
            "--video".into(),
        ]);
        let value: Value = serde_json::from_str(&text).unwrap();
        let note = value["device_note"].as_str().unwrap_or("");
        assert!(note.contains("CPU"), "{text}");
        assert!(
            value["heat_note"].as_str().unwrap_or("").contains("hot"),
            "{text}"
        );
    }

    #[test]
    fn a_broken_argument_list_refuses_before_a_scan() {
        let refuse = |args: &[&str]| {
            let owned: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
            let value: Value = serde_json::from_str(&invoke_argv(&owned)).unwrap();
            assert_eq!(value["status"], "refused", "{args:?} {value}");
            assert_ne!(value["summary"], "No candidate is not a clearance.");
        };
        refuse(&[]);
        refuse(&["dance"]);
        refuse(&["posters"]);
        refuse(&["posters", "write-fixture"]);
        refuse(&["--bundles"]);
        refuse(&["estimate", "--long-side", "100"]);
        refuse(&["estimate", "--form-factor", "tablet"]);
        refuse(&["estimate", "--coverage", "sometimes"]);
        refuse(&["scan", "extra"]);
        refuse(&["scan", "--input"]);
        refuse(&[
            "estimate",
            "--abort-after-frames",
            "nope",
            "--width",
            "10",
            "--height",
            "10",
            "--frame-count",
            "1",
        ]);
        refuse(&[
            "estimate",
            "--width",
            "0",
            "--height",
            "10",
            "--frame-count",
            "1",
        ]);
        refuse(&["estimate", "--video"]);
        refuse(&[
            "estimate",
            "--width",
            "8",
            "--height",
            "8",
            "--frame-count",
            "1",
            "--fps",
            "fast",
        ]);
        refuse(&[
            "estimate",
            "--width",
            "8",
            "--height",
            "8",
            "--frame-count",
            "1",
            "--duration",
            "fast",
        ]);
        refuse(&[
            "estimate",
            "--width",
            "8",
            "--height",
            "8",
            "--frame-count",
            "1",
            "--container-unix",
            "fast",
        ]);
        refuse(&[
            "posters",
            "check",
            "--posters",
            "/tmp/openworld-no-such-pack",
        ]);

        let dir = tempfile::tempdir().unwrap();
        let bundles = repo_bundles();
        let written: Value = serde_json::from_str(&invoke_argv(&[
            "--bundles".into(),
            bundles,
            "posters".into(),
            "write-fixture".into(),
            "--out".into(),
            dir.path().display().to_string(),
        ]))
        .unwrap();
        assert_eq!(written["perception"], "fiducial-v1");
        let checked: Value = serde_json::from_str(&invoke_argv(&[
            "posters".into(),
            "check".into(),
            "--posters".into(),
            dir.path().display().to_string(),
        ]))
        .unwrap();
        assert_eq!(checked["id"], "fixture-v0");
        assert!(checked["posters"].as_u64().unwrap() >= 1);
    }
}
