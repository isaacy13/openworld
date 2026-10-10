// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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

#[test]
fn a_bad_class_answer_stops_before_the_estimate() {
    let dir = tempfile::tempdir().unwrap();
    let still = dir.path().join("still.png");
    let (code, _text, err) = run(&["fixture-still", "--blank", "--out", still.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}");
    let posters = dir.path().join("posters");
    let (code, _text, err) = run(&[
        "posters",
        "write-fixture",
        "--out",
        posters.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{err}");
    let out = dir.path().join("result");
    let catalog = bundles();
    let args = [
        "--bundles",
        catalog.as_str(),
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
        "--out",
        out.to_str().unwrap(),
    ];

    let mut child = Command::new(bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("openworld");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"maybe\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    assert!(err.is_empty(), "{err}");
    assert!(text
        .lines()
        .any(|line| line == "Choose missing, wanted, or both."));
    assert!(
        text.contains("Missing? [Y/n]: \nChoose missing, wanted, or both.\n"),
        "{text:?}"
    );
    assert!(!text.contains("\n\nChoose missing, wanted, or both."), "{text:?}");
    assert!(!text.lines().any(|line| line == "Estimate"), "{text}");
    assert!(!text.contains("Wanted?"), "{text}");
    assert!(!text.contains("Analyze?"), "{text}");
    assert!(!out.join("result.json").exists());

    let mut child = Command::new(bin())
        .arg("--json")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("openworld");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"n\nmaybe\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{err}\n{text}");
    let doc: serde_json::Value = serde_json::from_str(text.trim()).expect(&text);
    assert_eq!(doc["status"], "refused");
    assert_eq!(doc["summary"], "Choose missing, wanted, or both.");
    assert_eq!(doc["message"], "Choose missing, wanted, or both.");
    assert!(err.contains("Wanted? [Y/n]:"), "{err}");
    assert!(!text.contains("This file stays on this device."), "{text}");
    assert!(!text.contains("Estimate"), "{text}");
    assert!(!out.join("result.json").exists());
}
