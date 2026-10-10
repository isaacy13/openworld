// SPDX-License-Identifier: Apache-2.0

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
