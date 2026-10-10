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

fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let output = Command::new(bin())
        .arg("--bundles")
        .arg(bundles())
        .args(args)
        .output()
        .expect("openworld");
    (
        output.status.code(),
        String::from_utf8(output.stdout).expect("stdout"),
        String::from_utf8(output.stderr).expect("stderr"),
    )
}

#[test]
fn estimate_and_analyze_name_the_class_that_is_on() {
    let dir = tempfile::tempdir().unwrap();
    let still = dir.path().join("still.png");
    let (code, _text, err) = run(&["fixture-still", "--blank", "--out", still.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}");

    let (code, text, err) = run(&[
        "estimate",
        "--input",
        still.to_str().unwrap(),
        "--no-wanted",
    ]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(text.lines().last(), Some("Missing."));
    assert!(text.lines().all(|line| line != "Missing and wanted."));

    let (code, text, err) = run(&[
        "estimate",
        "--input",
        still.to_str().unwrap(),
        "--no-missing",
        "--no-wanted",
    ]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        text.lines().last(),
        Some("Choose missing, wanted, or both.")
    );

    let posters = dir.path().join("posters");
    let (code, _text, err) = run(&[
        "posters",
        "write-fixture",
        "--out",
        posters.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{err}");

    let blocked = dir.path().join("blocked");
    let (code, text, err) = run(&[
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
        blocked.to_str().unwrap(),
        "--no-missing",
        "--no-wanted",
    ]);
    assert_eq!(code, Some(2), "{text}\n{err}");
    assert!(text
        .lines()
        .any(|line| line == "Choose missing, wanted, or both."));
    assert!(!blocked.join("result.json").exists());

    let out = dir.path().join("missing-only");
    let (code, text, err) = run(&[
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
        "--no-wanted",
    ]);
    assert_eq!(code, Some(0), "{text}\n{err}");
    assert!(text.lines().any(|line| line == "Missing."));
    assert!(text.lines().all(|line| line != "Missing and wanted."));
    let saved = std::fs::read(out.join("result.json")).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(report["class_note"], "Missing.");
}
