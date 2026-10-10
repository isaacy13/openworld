// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_openworld")
}

fn bundles() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bundles")
        .display()
        .to_string()
}

fn run(json: bool, args: &[&str]) -> (Option<i32>, String, String) {
    let mut cmd = Command::new(bin());
    if json {
        cmd.arg("--json");
    }
    cmd.arg("--bundles").arg(bundles()).args(args);
    let output = cmd.output().expect("openworld");
    (
        output.status.code(),
        String::from_utf8(output.stdout).expect("stdout"),
        String::from_utf8(output.stderr).expect("stderr"),
    )
}

#[test]
fn a_scan_says_scanning_and_names_each_crop() {
    let dir = tempfile::tempdir().unwrap();
    let demo = dir.path().join("demo");
    let (code, text, err) = run(false, &["demo", "--out", demo.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}\n{text}");
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.first(), Some(&"Scanning"));
    let crops = lines
        .iter()
        .position(|line| *line == "Crops from this file.")
        .expect("crops");
    let summary = lines
        .iter()
        .position(|line| *line == "Possible candidate. Not an identification.")
        .expect("summary");
    assert!(crops < summary, "{lines:?}");
    assert_eq!(lines[crops + 1], "Frame 1.");
    assert!(!lines[crops + 2].is_empty());
    assert!(
        lines[crops + 3].starts_with("crops/"),
        "{}",
        lines[crops + 3]
    );
    assert!(lines.iter().all(|line| {
        !line.starts_with("face ") && !line.starts_with("plate ") && !line.starts_with("vehicle ")
    }));
    let report_crops = lines
        .iter()
        .rposition(|line| *line == "Crops from this file.")
        .expect("report crops");
    assert!(summary < report_crops);

    let (code, json, err) = run(
        true,
        &["demo", "--out", dir.path().join("json").to_str().unwrap()],
    );
    assert_eq!(code, Some(0), "{err}");
    assert!(json.trim_start().starts_with('{'), "{json}");
    assert!(json.lines().all(|line| line != "Scanning"));

    let still = dir.path().join("blank.png");
    let (code, _text, err) = run(
        false,
        &["fixture-still", "--blank", "--out", still.to_str().unwrap()],
    );
    assert_eq!(code, Some(0), "{err}");
    let posters = dir.path().join("posters");
    let (code, _text, err) = run(
        false,
        &[
            "posters",
            "write-fixture",
            "--out",
            posters.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{err}");
    let out = dir.path().join("blank-result");
    let (code, text, err) = run(
        false,
        &[
            "scan",
            "--input",
            still.to_str().unwrap(),
            "--posters",
            posters.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{err}\n{text}");
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.first(), Some(&"Scanning"));
    assert_eq!(lines.get(1), Some(&"No candidate is not a clearance."));
    assert!(lines.iter().all(|line| *line != "Crops from this file."));
}

#[test]
fn an_old_file_is_warned_before_the_estimate() {
    let dir = tempfile::tempdir().unwrap();
    let still = dir.path().join("still.png");
    let (code, _text, err) = run(
        false,
        &["fixture-still", "--blank", "--out", still.to_str().unwrap()],
    );
    assert_eq!(code, Some(0), "{err}");
    let fresh = run(false, &["estimate", "--input", still.to_str().unwrap()]);
    assert_eq!(fresh.0, Some(0), "{}", fresh.2);
    assert!(fresh
        .1
        .lines()
        .all(|line| line != "This file is older than about 30 days."));

    let file = std::fs::File::options().write(true).open(&still).unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(40 * 24 * 3600);
    file.set_modified(old).unwrap();
    drop(file);
    let (code, text, err) = run(false, &["estimate", "--input", still.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}");
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.first(), Some(&"Estimate"));
    assert_eq!(
        lines.get(1),
        Some(&"This file is older than about 30 days.")
    );
    let caveat = lines
        .iter()
        .position(|line| *line == "This is a planning estimate, not a thermal measurement.")
        .expect("caveat");
    assert!(caveat > 1);
    assert!(lines
        .iter()
        .all(|line| *line != "The file timestamps disagree."));

    let posters = dir.path().join("posters");
    let (code, _text, err) = run(
        false,
        &[
            "posters",
            "write-fixture",
            "--out",
            posters.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{err}");
    let out = dir.path().join("result");
    let (code, text, err) = run(
        false,
        &[
            "analyze",
            "--yes",
            "--input",
            still.to_str().unwrap(),
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            "complete",
            "--posters",
            posters.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{err}\n{text}");
    let lines: Vec<_> = text.lines().collect();
    let warning = "This file is older than about 30 days.";
    let first = lines
        .iter()
        .position(|line| *line == warning)
        .expect("file warning");
    let uploaded = lines
        .iter()
        .position(|line| *line == "Nothing is uploaded.")
        .expect("disclosure");
    let second = lines
        .iter()
        .rposition(|line| *line == warning)
        .expect("later warning");
    let fixture = lines
        .iter()
        .position(|line| *line == "Fixture posters. Real FBI photos stay off.")
        .expect("fixture");
    assert!(
        first < uploaded && uploaded < fixture && fixture < second,
        "{lines:?}"
    );
    assert_eq!(lines[first - 1], "still.png");
    let saved = std::fs::read(out.join("result.json")).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    let warnings = report["warnings"].as_array().unwrap();
    assert!(warnings.iter().any(|line| line == warning));
}

#[test]
fn analyze_json_is_one_document() {
    let dir = tempfile::tempdir().unwrap();
    let still = dir.path().join("blank.png");
    let (code, _text, err) = run(
        false,
        &["fixture-still", "--blank", "--out", still.to_str().unwrap()],
    );
    assert_eq!(code, Some(0), "{err}");
    let posters = dir.path().join("posters");
    let (code, _text, err) = run(
        false,
        &[
            "posters",
            "write-fixture",
            "--out",
            posters.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{err}");
    let args = [
        "analyze",
        "--input",
        still.to_str().unwrap(),
        "--bundle",
        "fast",
        "--long-side",
        "640",
        "--coverage",
        "complete",
        "--posters",
        posters.to_str().unwrap(),
    ];

    let held = dir.path().join("held");
    let (code, text, err) = run(
        true,
        &[args.as_slice(), &["--out", held.to_str().unwrap()]]
            .concat()
            .as_slice(),
    );
    assert_eq!(code, Some(0), "{err}\n{text}");
    let held_doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(held_doc["started"], false);
    assert_eq!(held_doc["message"], "Not started.");
    assert!(!held.join("result.json").exists());

    let out = dir.path().join("result");
    let (code, text, err) = run(
        true,
        &[
            args.as_slice(),
            &["--yes", "--out", out.to_str().unwrap()],
        ]
        .concat()
        .as_slice(),
    );
    assert_eq!(code, Some(0), "{err}\n{text}");
    let report: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(report["status"], "complete");
    assert_eq!(report["summary"], "No candidate is not a clearance.");
    assert!(text.lines().all(|line| line != "Scanning"));
    assert!(text.lines().all(|line| line != "This file stays on this device."));

    let blocked = dir.path().join("blocked");
    let (code, text, err) = run(
        true,
        &[
            args.as_slice(),
            &[
                "--yes",
                "--no-missing",
                "--no-wanted",
                "--out",
                blocked.to_str().unwrap(),
            ],
        ]
        .concat()
        .as_slice(),
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    let refusal: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(refusal["status"], "refused");
    assert_eq!(refusal["message"], "Choose missing, wanted, or both.");
    assert!(!blocked.join("result.json").exists());
}

#[test]
fn analyze_yes_uses_the_preselected_choices() {
    let dir = tempfile::tempdir().unwrap();
    let still = dir.path().join("blank.png");
    let (code, _text, err) = run(
        false,
        &["fixture-still", "--blank", "--out", still.to_str().unwrap()],
    );
    assert_eq!(code, Some(0), "{err}");
    let posters = dir.path().join("posters");
    let (code, _text, err) = run(
        false,
        &[
            "posters",
            "write-fixture",
            "--out",
            posters.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{err}");
    let out = dir.path().join("result");
    let (code, text, err) = run(
        false,
        &[
            "analyze",
            "--yes",
            "--input",
            still.to_str().unwrap(),
            "--posters",
            posters.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{err}\n{text}");
    assert!(!text.contains("Model bundle [fast]:"));
    assert!(!text.contains("Detection size (320"));
    assert!(!text.contains("Coverage (complete"));
    assert!(!err.contains("Model bundle [fast]:"));
    assert!(text.lines().any(|line| line == "Estimate"));
    let saved = std::fs::read(out.join("result.json")).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(report["bundle_name"], "Fast");
    assert_eq!(report["detection_note"], "640 px on the long side.");
    assert_eq!(report["coverage_note"], "Every decoded frame.");
    assert_eq!(report["class_note"], "Missing and wanted.");
}

#[test]
fn a_missing_file_is_not_a_bad_codec() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-photo.png");
    let folder = dir.path().join("not-a-file");
    fs::create_dir(&folder).unwrap();
    let blocked = dir.path().join("unreadable.bin");
    fs::write(&blocked, b"x").unwrap();
    let mut perms = fs::metadata(&blocked).unwrap().permissions();
    perms.set_mode(0);
    fs::set_permissions(&blocked, perms).unwrap();
    for path in [&missing, &folder, &blocked] {
        let (code, text, err) = run(
            false,
            &[
                "estimate",
                "--input",
                path.to_str().unwrap(),
                "--bundle",
                "fast",
                "--long-side",
                "640",
                "--coverage",
                "complete",
            ],
        );
        assert_eq!(code, Some(2), "{err}\n{text}");
        assert_eq!(text.lines().next(), Some("The file could not be read. Refusing."), "{text}");
        assert!(!text.contains("Bad codec"));
        assert!(!text.contains("This file stays on this device."));
    }
    let junk = dir.path().join("notes.txt");
    fs::write(&junk, b"not a photo\n").unwrap();
    let (code, text, err) = run(
        false,
        &[
            "estimate",
            "--input",
            junk.to_str().unwrap(),
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            "complete",
        ],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert!(text.contains("Bad codec or unreadable file. Refusing."), "{text}");
    let (code, text, err) = run(
        false,
        &[
            "analyze",
            "--yes",
            "--input",
            missing.to_str().unwrap(),
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            "complete",
        ],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some("The file could not be read. Refusing."), "{text}");
    assert!(!text.contains("This file stays on this device."));
    assert!(!err.contains("Poster pack"));
    let (code, text, err) = run(
        true,
        &[
            "analyze",
            "--yes",
            "--input",
            missing.to_str().unwrap(),
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            "complete",
        ],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["message"], "The file could not be read. Refusing.");
    assert!(!text.contains("This file stays on this device."));
}

#[test]
fn an_unreadable_catalog_is_a_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-catalog");
    let mut cmd = Command::new(bin());
    cmd.arg("--bundles").arg(&missing).arg("bundles");
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    assert_eq!(text.trim(), "The bundle catalog could not be read. Refusing.");
    assert!(err.is_empty(), "{err}");

    let mut cmd = Command::new(bin());
    cmd.arg("--json").arg("--bundles").arg(&missing).arg("bundles");
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["message"], "The bundle catalog could not be read. Refusing.");
    assert_eq!(doc["summary"], "The bundle catalog could not be read. Refusing.");
    assert!(doc.get("bundles").is_none());

    let input = dir.path().join("notes.txt");
    fs::write(&input, b"not a photo").unwrap();
    let mut cmd = Command::new(bin());
    cmd.arg("--bundles")
        .arg(&missing)
        .args(["analyze", "--yes", "--input"])
        .arg(&input)
        .args(["--bundle", "fast", "--long-side", "640", "--coverage", "complete"]);
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    assert_eq!(
        text.lines().next(),
        Some("The bundle catalog could not be read. Refusing."),
        "{text}"
    );
    assert!(!text.contains("This file stays on this device."), "{text}");
}

#[test]
fn a_bad_size_or_an_empty_catalog_stops_before_the_poster_pack() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("notes.txt");
    fs::write(&input, b"not a photo").unwrap();
    let empty = dir.path().join("empty-bundles");
    fs::create_dir(&empty).unwrap();

    let mut cmd = Command::new(bin());
    cmd.arg("--bundles")
        .arg(&empty)
        .args(["analyze", "--yes", "--input"])
        .arg(&input)
        .args(["--bundle", "fast", "--long-side", "640", "--coverage", "complete"]);
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    assert!(text.contains("This file stays on this device."), "{text}");
    let device = text
        .lines()
        .position(|line| line == "This file stays on this device.")
        .expect("device");
    let refusal = text
        .lines()
        .position(|line| line == "The scan program is not on this device. Refusing.")
        .expect("refusal");
    assert!(device < refusal, "{text}");
    assert!(!text.lines().any(|line| line == "Detection size"), "{text}");
    assert!(!text.contains("Poster pack directory:"), "{text}");

    let mut cmd = Command::new(bin());
    cmd.arg("--json")
        .arg("--bundles")
        .arg(bundles())
        .args(["analyze", "--yes", "--input"])
        .arg(&input)
        .args(["--bundle", "fast", "--long-side", "999", "--coverage", "complete"]);
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(
        doc["message"],
        "Detection size must be 320, 480, 640, or full. Refusing."
    );
    assert!(!text.contains("Poster pack directory:"), "{text}");
    assert!(!text.contains("This file stays on this device."), "{text}");

    let mut cmd = Command::new(bin());
    cmd.arg("--bundles")
        .arg(bundles())
        .args(["analyze", "--yes", "--input"])
        .arg(&input)
        .args(["--bundle", "nope", "--long-side", "640", "--coverage", "weekly"]);
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    assert!(text.contains("That bundle is not in the catalog. Refusing."), "{text}");
    assert!(!text.contains("Coverage must be complete or measured. Refusing."), "{text}");
    assert!(!text.contains("Poster pack directory:"), "{text}");

    let mut cmd = Command::new(bin());
    cmd.arg("--json").args([
        "estimate",
        "--input",
        "missing.png",
        "--bundle",
        "fast",
        "--long-side",
        "640",
        "--coverage",
        "weekly",
    ]);
    cmd.arg("--bundles").arg(bundles());
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["message"], "Coverage must be complete or measured. Refusing.");
    assert!(err.is_empty(), "{err}");
}

#[test]
fn a_missing_poster_pack_stops_before_the_estimate() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("notes.txt");
    fs::write(&input, b"not a photo").unwrap();
    let missing = dir.path().join("no-pack");
    let mut cmd = Command::new(bin());
    cmd.arg("--bundles")
        .arg(bundles())
        .args(["analyze", "--yes", "--input"])
        .arg(&input)
        .args([
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            "complete",
            "--posters",
        ])
        .arg(&missing);
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    let device = text
        .lines()
        .position(|line| line == "This file stays on this device.")
        .expect("device");
    let refusal = text
        .lines()
        .position(|line| line == "The poster pack is missing. Refusing.")
        .expect("refusal");
    assert!(device < refusal, "{text}");
    assert!(!text.lines().any(|line| line == "Estimate"), "{text}");
    assert!(!text.contains("Result directory:"), "{text}");
    assert!(!text.contains("Analyze? [y/N]:"), "{text}");

    let mut cmd = Command::new(bin());
    cmd.arg("--json")
        .arg("--bundles")
        .arg(bundles())
        .args(["analyze", "--yes", "--input"])
        .arg(&input)
        .args([
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            "complete",
            "--posters",
        ])
        .arg(&missing);
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "missing_pack");
    assert_eq!(doc["message"], "The poster pack is missing. Refusing.");
    assert!(!text.contains("This file stays on this device."), "{text}");

    let blocked = dir.path().join("blocked");
    fs::write(&blocked, b"not a directory").unwrap();
    let mut cmd = Command::new(bin());
    cmd.args(["posters", "write-fixture", "--out"]).arg(&blocked);
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    assert_eq!(text.trim(), "The poster pack could not be read. Refusing.");
    assert!(err.is_empty(), "{err}");
}

#[test]
fn a_result_file_stops_before_the_estimate() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("blank.png");
    let (code, text, err) = run(
        false,
        &["fixture-still", "--blank", "--out", input.to_str().unwrap()],
    );
    assert_eq!(code, Some(0), "{err}\n{text}");
    let posters = dir.path().join("posters");
    let (code, text, err) = run(
        false,
        &["posters", "write-fixture", "--out", posters.to_str().unwrap()],
    );
    assert_eq!(code, Some(0), "{err}\n{text}");
    let blocked = dir.path().join("result-file");
    fs::write(&blocked, b"keep").unwrap();

    let (code, text, err) = run(
        false,
        &[
            "analyze",
            "--yes",
            "--input",
            input.to_str().unwrap(),
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            "complete",
            "--posters",
            posters.to_str().unwrap(),
            "--out",
            blocked.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    let device = text
        .lines()
        .position(|line| line == "This file stays on this device.")
        .expect("device");
    let refusal = text
        .lines()
        .position(|line| line == "The output directory could not be created. Refusing.")
        .expect("refusal");
    assert!(device < refusal, "{text}");
    assert!(!text.lines().any(|line| line == "Estimate"), "{text}");
    assert!(!text.contains("Missing and wanted."), "{text}");
    assert!(!text.contains("Analyze? [y/N]:"), "{text}");
    assert!(!text.contains("Scanning"), "{text}");
    assert_eq!(fs::read(&blocked).unwrap(), b"keep");

    let nested = blocked.join("out");
    let (code, text, err) = run(
        false,
        &[
            "analyze",
            "--yes",
            "--input",
            input.to_str().unwrap(),
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            "complete",
            "--posters",
            posters.to_str().unwrap(),
            "--out",
            nested.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert!(
        text.contains("The output directory could not be created. Refusing."),
        "{text}"
    );
    assert!(!text.lines().any(|line| line == "Estimate"), "{text}");
    assert_eq!(fs::read(&blocked).unwrap(), b"keep");

    let (code, text, err) = run(
        true,
        &[
            "analyze",
            "--yes",
            "--input",
            input.to_str().unwrap(),
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            "complete",
            "--posters",
            posters.to_str().unwrap(),
            "--out",
            blocked.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "unreadable");
    assert_eq!(
        doc["message"],
        "The output directory could not be created. Refusing."
    );
    assert!(!text.contains("This file stays on this device."), "{text}");
    assert_eq!(fs::read(&blocked).unwrap(), b"keep");

    let kept = dir.path().join("kept");
    fs::create_dir(&kept).unwrap();
    fs::write(kept.join("keep.txt"), b"stay").unwrap();
    let mut cmd = Command::new(bin());
    cmd.arg("--bundles").arg(bundles()).args([
        "analyze",
        "--input",
        input.to_str().unwrap(),
        "--bundle",
        "fast",
        "--long-side",
        "640",
        "--coverage",
        "complete",
        "--no-wanted",
        "--posters",
        posters.to_str().unwrap(),
        "--out",
        kept.to_str().unwrap(),
    ]);
    cmd.stdin(std::process::Stdio::null());
    let output = cmd.output().expect("openworld");
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(0), "{err}\n{text}");
    assert!(text.lines().any(|line| line == "Estimate"), "{text}");
    assert!(text.contains("Not started."), "{text}");
    assert!(!text.contains("Scanning"), "{text}");
    assert_eq!(fs::read(kept.join("keep.txt")).unwrap(), b"stay");
}

#[test]
fn measure_demo_and_a_fixture_still_use_the_catalog_sentence() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let missing = "The scan program is not on this device. Refusing.";

    let (code, text, err) = catalog_run(false, &empty, &["measure"]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(missing), "{text}");
    assert!(err.is_empty(), "{err}");
    assert!(!text.contains("Fast bundle is missing."), "{text}");

    let (code, text, err) = catalog_run(true, &empty, &["measure"]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(&text).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "missing_program");
    assert_eq!(doc["summary"], missing);
    assert_eq!(doc["message"], missing);
    assert!(err.is_empty(), "{err}");

    let demo = dir.path().join("demo");
    let (code, text, err) = catalog_run(
        false,
        &empty,
        &["demo", "--out", demo.to_str().unwrap()],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(missing), "{text}");
    assert!(!text.contains("Scanning"), "{text}");
    assert!(!demo.exists());

    let scene = dir.path().join("scenes").join("still.png");
    let (code, text, err) = catalog_run(
        false,
        &empty,
        &["fixture-still", "--scene", "--out", scene.to_str().unwrap()],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(missing), "{text}");
    assert!(!scene.exists());
    assert!(!scene.parent().unwrap().exists());

    let marker = dir.path().join("markers").join("face.png");
    let (code, text, err) = catalog_run(
        true,
        &empty,
        &["fixture-still", "--out", marker.to_str().unwrap()],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(&text).expect(&text);
    assert_eq!(doc["refusal"], "missing_program");
    assert_eq!(doc["message"], missing);
    assert!(!marker.exists());

    let blank = dir.path().join("blank.png");
    let (code, text, err) = catalog_run(
        false,
        &empty,
        &["fixture-still", "--blank", "--out", blank.to_str().unwrap()],
    );
    assert_eq!(code, Some(0), "{err}\n{text}");
    assert!(blank.is_file());

    let accurate = dir.path().join("accurate-only");
    let bundle = accurate.join("accurate");
    fs::create_dir_all(&bundle).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundles/accurate/manifest.toml"),
        bundle.join("manifest.toml"),
    )
    .unwrap();
    let absent = "That bundle is not in the catalog. Refusing.";
    let demo = dir.path().join("accurate-demo");
    let (code, text, err) = catalog_run(
        true,
        &accurate,
        &["demo", "--out", demo.to_str().unwrap()],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(&text).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "bundle_not_found");
    assert_eq!(doc["summary"], absent);
    assert_eq!(doc["message"], absent);
    assert!(!demo.exists());
    assert!(!text.contains("Fast bundle is missing."), "{text}");

    let unreadable = dir.path().join("not-a-catalog");
    fs::write(&unreadable, b"keep").unwrap();
    let (code, text, err) = catalog_run(true, &unreadable, &["measure"]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(&text).expect(&text);
    assert_eq!(doc["refusal"], "catalog");
    assert_eq!(doc["message"], "The bundle catalog could not be read. Refusing.");
    assert_eq!(fs::read(&unreadable).unwrap(), b"keep");
}

#[test]
fn a_zero_size_platform_decode_is_the_bad_codec_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("result");
    let sentence = "Bad codec or unreadable file. Refusing.";
    let (code, text, err) = run(
        false,
        &[
            "estimate",
            "--input",
            "missing.png",
            "--width",
            "0",
            "--height",
            "10",
            "--frame-count",
            "1",
        ],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(sentence), "{text}");
    assert!(err.is_empty(), "{err}");
    assert!(!text.lines().any(|line| line == "Estimate"), "{text}");

    let (code, text, err) = run(
        true,
        &[
            "scan",
            "--input",
            "missing.png",
            "--width",
            "8",
            "--height",
            "0",
            "--frame-count",
            "2",
            "--out",
            out.to_str().unwrap(),
            "--posters",
            dir.path().join("posters").to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(&text).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "bad_codec");
    assert_eq!(doc["summary"], sentence);
    assert_eq!(doc["message"], sentence);
    assert!(err.is_empty(), "{err}");
    assert!(!text.contains("Scanning"), "{text}");
    assert!(!out.exists());

    let (code, text, err) = run(
        true,
        &[
            "estimate",
            "--input",
            "missing.png",
            "--video",
            "--height",
            "10",
            "--frame-count",
            "1",
        ],
    );
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(&text).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "bad_request");
    assert_eq!(doc["message"], "A platform decode needs a width.");
    assert!(err.is_empty(), "{err}");
}

#[test]
fn an_empty_catalog_names_the_missing_program_on_bundles() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let sentence = "The scan program is not on this device. Refusing.";
    let (code, text, err) = catalog_run(false, &empty, &["bundles"]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.first().copied(), Some("Model bundle"));
    assert_eq!(
        lines.iter().filter(|line| **line == sentence).count(),
        1,
        "{text}"
    );
    assert!(err.is_empty(), "{err}");

    let (code, text, err) = catalog_run(true, &empty, &["bundles"]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(&text).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "missing_program");
    assert_eq!(doc["summary"], sentence);
    assert_eq!(doc["message"], sentence);
    assert!(doc.get("bundles").is_none());
    assert!(!text.contains("Model bundle"), "{text}");
    assert!(err.is_empty(), "{err}");
}

#[test]
fn a_poster_update_prints_the_refusal() {
    let sentence = "Real FBI photos stay off. Fast does not have a curve that allows them.";
    let (code, text, err) = run(false, &["posters", "update"]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.trim(), sentence);
    assert!(err.is_empty(), "{err}");

    let (code, text, err) = run(true, &["posters", "update"]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(&text).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["summary"], sentence);
    assert_eq!(doc["message"], sentence);
    assert!(doc.get("detail").is_none(), "{text}");
    assert!(err.is_empty(), "{err}");
}

#[test]
fn a_demo_file_refuses_before_it_scans() {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("demo-file");
    fs::write(&blocked, b"keep").unwrap();
    let created = "The output directory could not be created. Refusing.";

    let (code, text, err) = run(false, &["demo", "--out", blocked.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(created), "{text}");
    assert!(!text.contains("Scanning"), "{text}");
    assert!(!text.contains("No candidate is not a clearance."), "{text}");
    assert!(err.is_empty(), "{err}");
    assert_eq!(fs::read(&blocked).unwrap(), b"keep");

    let nested = blocked.join("demo");
    let (code, text, err) = run(false, &["demo", "--out", nested.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(created), "{text}");
    assert!(!text.contains("Scanning"), "{text}");
    assert_eq!(fs::read(&blocked).unwrap(), b"keep");
    assert!(!nested.exists());

    let (code, text, err) = run(true, &["demo", "--out", blocked.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "unreadable");
    assert_eq!(doc["summary"], created);
    assert_eq!(doc["message"], created);
    assert!(err.is_empty(), "{err}");
    assert_eq!(fs::read(&blocked).unwrap(), b"keep");

    let out = dir.path().join("demo-dir");
    fs::create_dir(&out).unwrap();
    let posters = out.join("posters");
    fs::write(&posters, b"keep-pack").unwrap();
    let pack = "The poster pack could not be read. Refusing.";
    let (code, text, err) = run(false, &["demo", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(pack), "{text}");
    assert!(!text.contains("Scanning"), "{text}");
    assert!(!text.contains("No candidate is not a clearance."), "{text}");
    assert!(err.is_empty(), "{err}");
    assert_eq!(fs::read(&posters).unwrap(), b"keep-pack");

    let (code, text, err) = run(true, &["demo", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "bad_hash");
    assert_eq!(doc["summary"], pack);
    assert_eq!(doc["message"], pack);
    assert!(err.is_empty(), "{err}");
    assert_eq!(fs::read(&posters).unwrap(), b"keep-pack");
}

#[test]
fn a_still_that_cannot_be_written_uses_the_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let still = "The still could not be written. Refusing.";
    let created = "The output directory could not be created. Refusing.";

    let out = dir.path().join("demo");
    fs::create_dir(&out).unwrap();
    let input = out.join("input.png");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("keep.txt"), b"keep").unwrap();
    let (code, text, err) = run(false, &["demo", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(still), "{text}");
    assert!(!text.contains("Scanning"), "{text}");
    assert!(!text.contains("No candidate is not a clearance."), "{text}");
    assert!(err.is_empty(), "{err}");
    assert!(input.is_dir());
    assert_eq!(fs::read(input.join("keep.txt")).unwrap(), b"keep");
    assert!(!out.join("posters").exists());

    let (code, text, err) = run(true, &["demo", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "unreadable");
    assert_eq!(doc["summary"], still);
    assert_eq!(doc["message"], still);
    assert!(err.is_empty(), "{err}");

    let folder = dir.path().join("folder");
    fs::create_dir(&folder).unwrap();
    let (code, text, err) = run(false, &["fixture-still", "--blank", "--out", folder.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(still), "{text}");
    assert!(err.is_empty(), "{err}");
    assert!(folder.is_dir());
    assert!(fs::read_dir(&folder).unwrap().next().is_none());

    let blocker = dir.path().join("not-a-dir");
    fs::write(&blocker, b"keep").unwrap();
    let nested = blocker.join("still.png");
    let (code, text, err) = run(true, &["fixture-still", "--blank", "--out", nested.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["refusal"], "unreadable");
    assert_eq!(doc["summary"], created);
    assert_eq!(doc["message"], created);
    assert!(err.is_empty(), "{err}");
    assert_eq!(fs::read(&blocker).unwrap(), b"keep");
    assert!(!nested.exists());

    let scene = dir.path().join("scene-dir");
    fs::create_dir(&scene).unwrap();
    let (code, text, err) = run(false, &["fixture-still", "--scene", "--out", scene.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}\n{text}");
    assert_eq!(text.lines().next(), Some(still), "{text}");
    assert!(!text.contains("Scanning"), "{text}");
    assert!(err.is_empty(), "{err}");
    assert!(scene.is_dir());
}

#[test]
fn a_fixture_still_answer_is_a_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("still.png");
    let cases = [
        (
            vec!["fixture-still", "--scene", "--blank", "--out", out.to_str().unwrap()],
            "Choose a scene or a blank still.",
        ),
        (
            vec!["fixture-still", "--module", "0", "--out", out.to_str().unwrap()],
            "Module size must be at least 1.",
        ),
        (
            vec!["fixture-still", "--x", "8", "--out", out.to_str().unwrap()],
            "Provide both x and y, or neither.",
        ),
        (
            vec![
                "fixture-still",
                "--below-cutoff",
                "--x",
                "8",
                "--y",
                "8",
                "--out",
                out.to_str().unwrap(),
            ],
            "A below-cutoff still picks its own origin.",
        ),
        (
            vec![
                "fixture-still",
                "--x",
                "390",
                "--y",
                "300",
                "--out",
                out.to_str().unwrap(),
            ],
            "The marker does not fit on the still.",
        ),
    ];
    for (args, sentence) in cases {
        let (code, text, err) = run(false, &args);
        assert_eq!(code, Some(2), "{sentence}\n{err}\n{text}");
        assert_eq!(text.trim(), sentence, "{err}");
        assert!(err.is_empty(), "{err}");
        assert!(!out.exists(), "{sentence}");

        let (code, text, err) = run(true, &args);
        assert_eq!(code, Some(2), "{sentence}\n{err}\n{text}");
        let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
        assert_eq!(doc["status"], "refused");
        assert_eq!(doc["summary"], sentence);
        assert_eq!(doc["message"], sentence);
        assert!(doc.get("detail").is_none(), "{text}");
        assert!(err.is_empty(), "{err}");
        assert!(!out.exists(), "{sentence}");
    }
}

fn catalog_run(json: bool, bundles: &std::path::Path, args: &[&str]) -> (Option<i32>, String, String) {
    let mut cmd = Command::new(bin());
    if json {
        cmd.arg("--json");
    }
    cmd.arg("--bundles").arg(bundles).args(args);
    let output = cmd.output().expect("openworld");
    (
        output.status.code(),
        String::from_utf8(output.stdout).expect("stdout"),
        String::from_utf8(output.stderr).expect("stderr"),
    )
}
