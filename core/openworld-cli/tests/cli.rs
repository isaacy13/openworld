// SPDX-License-Identifier: Apache-2.0

//! The `openworld` program: each command, its JSON answer, its exit code.

use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn bundles() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundles")
}

fn run(args: &[&str], stdin: Option<&str>) -> (i32, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_openworld"))
        .arg("--bundles")
        .arg(bundles())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("openworld runs");
    child.stdin.take().unwrap().write_all(stdin.unwrap_or("").as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    (output.status.code().unwrap_or(-1), String::from_utf8(output.stdout).unwrap())
}

fn json(args: &[&str]) -> (i32, Value) {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    let (code, text) = run(&all, None);
    (code, serde_json::from_str(text.trim()).unwrap_or_else(|_| panic!("not JSON: {text}")))
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// A demo pack and a demo still in a fresh folder.
fn demo_inputs(root: &Path) -> (PathBuf, PathBuf) {
    let posters = root.join("posters");
    let still = root.join("still.png");
    assert_eq!(json(&["posters", "demo", "--out", s(&posters)]).0, 0);
    assert_eq!(json(&["fixture-still", "--out", s(&still), "--scene"]).0, 0);
    (posters, still)
}

#[test]
fn the_demo_finds_its_marker_and_says_it_is_a_demo() {
    let root = tempfile::tempdir().unwrap();
    let (code, text) = run(&["demo", "--out", s(&root.path().join("demo"))], None);
    assert_eq!(code, 0, "{text}");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Scanning");
    assert!(lines.contains(&"Possible candidate. Not an identification."));
    assert!(lines.contains(&"Demo posters. These are synthetic markers, not faces."));
    assert!(lines.contains(&"You are leaving OpenWorld."));
    assert!(lines.contains(&"No candidate is not a clearance."), "the disclosure keeps the clearance line");
    // A demo into a folder that is not empty refuses and keeps that folder.
    let (code, text) = run(&["demo", "--out", s(root.path())], None);
    assert_eq!(code, 2);
    assert!(text.contains("not an OpenWorld result"), "{text}");
}

#[test]
fn a_scan_writes_one_row_per_face_and_never_empties_a_foreign_folder() {
    let root = tempfile::tempdir().unwrap();
    let (posters, still) = demo_inputs(root.path());
    let out = root.path().join("result");
    let (code, report) = json(&["scan", "--input", s(&still), "--posters", s(&posters), "--out", s(&out)]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["schema"], "openworld.result.v2");
    assert_eq!(report["status"], "complete");
    assert_eq!(report["tracks"].as_array().unwrap().len(), 2);
    assert_eq!(report["candidates"][0]["poster"]["class"], "missing");
    assert!(report["candidates"][0].get("score").is_none(), "no raw score leaves the library");
    assert!(out.join("result.json").is_file());
    // Scanning into the same result replaces it.
    assert_eq!(json(&["scan", "--input", s(&still), "--posters", s(&posters), "--out", s(&out)]).0, 0);
    // A folder that holds something else refuses.
    let precious = root.path().join("precious");
    std::fs::create_dir_all(&precious).unwrap();
    std::fs::write(precious.join("thesis.docx"), b"years").unwrap();
    let (code, report) = json(&["scan", "--input", s(&still), "--posters", s(&posters), "--out", s(&precious)]);
    assert_eq!((code, report["refusal"].as_str()), (2, Some("out_foreign")));
    assert!(precious.join("thesis.docx").is_file());
    // A result folder that would hold the input refuses.
    let (code, report) = json(&["scan", "--input", s(&still), "--posters", s(&posters), "--out", s(root.path())]);
    assert_eq!((code, report["refusal"].as_str()), (2, Some("out_overlap")));
    assert!(still.is_file());
}

#[test]
fn both_classes_off_does_not_start() {
    let root = tempfile::tempdir().unwrap();
    let (posters, still) = demo_inputs(root.path());
    let (code, report) = json(&["scan", "--input", s(&still), "--posters", s(&posters), "--out", s(&root.path().join("r")), "--no-missing", "--no-wanted"]);
    assert_eq!((code, report["refusal"].as_str()), (2, Some("choose_class")));
    let (code, text) = run(&["analyze", "--input", s(&still), "--posters", s(&posters), "--out", s(&root.path().join("r")), "--no-missing", "--no-wanted", "--yes"], None);
    assert_eq!(code, 2);
    assert!(text.contains("Choose missing, wanted, or both."), "{text}");
}

#[test]
fn analyze_asks_before_it_starts() {
    let root = tempfile::tempdir().unwrap();
    let (posters, still) = demo_inputs(root.path());
    let out = root.path().join("r");
    let args = ["analyze", "--input", s(&still), "--posters", s(&posters), "--out", s(&out)];
    let (code, text) = run(&args, Some("n\n"));
    assert_eq!(code, 0);
    assert!(text.contains("Nothing is uploaded.") && text.contains("Estimate"), "{text}");
    assert!(text.trim_end().ends_with("Not started."), "{text}");
    assert!(!out.exists());
    let (code, text) = run(&args, Some("yes\n"));
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("Possible candidate. Not an identification."));
}

#[test]
fn refusals_name_their_reason() {
    let root = tempfile::tempdir().unwrap();
    let (posters, still) = demo_inputs(root.path());
    let missing = root.path().join("none.png");
    let (code, report) = json(&["scan", "--input", s(&missing), "--posters", s(&posters), "--out", s(&root.path().join("a"))]);
    assert_eq!((code, report["refusal"].as_str()), (2, Some("unreadable")));
    let text_file = root.path().join("notes.txt");
    std::fs::write(&text_file, b"not a picture").unwrap();
    let (code, report) = json(&["scan", "--input", s(&text_file), "--posters", s(&posters), "--out", s(&root.path().join("b"))]);
    assert_eq!((code, report["refusal"].as_str()), (2, Some("bad_codec")));
    let (code, report) = json(&["scan", "--input", s(&still), "--posters", s(&root.path().join("no-pack")), "--out", s(&root.path().join("c"))]);
    assert_eq!((code, report["refusal"].as_str()), (2, Some("missing_pack")));
    let (code, report) = json(&["scan", "--input", s(&still), "--posters", s(&posters), "--out", s(&root.path().join("d")), "--bundle", "nope"]);
    assert_eq!((code, report["refusal"].as_str()), (2, Some("bundle_not_found")));
    let (code, report) = json(&["scan", "--input", s(&still), "--posters", s(&posters), "--out", s(&root.path().join("e")), "--coverage", "some"]);
    assert_eq!((code, report["refusal"].as_str()), (2, Some("bad_coverage")));
    let (code, report) = json(&["posters", "update"]);
    assert_eq!((code, report["refusal"].as_str()), (2, Some("real_posters_off")));
}

#[test]
fn bundles_estimate_leave_delete_sweep_and_copy() {
    let (code, rows) = json(&["bundles"]);
    assert_eq!(code, 0);
    assert_eq!(rows["bundles"][0]["id"], "fast");
    assert_eq!(rows["bundles"][0]["preselected"], true);
    assert!(rows["bundles"].as_array().unwrap().iter().any(|b| b["id"] == "accurate" && b["curve_line"]["code"] == "not_measured"));
    let root = tempfile::tempdir().unwrap();
    let (posters, still) = demo_inputs(root.path());
    let (code, estimate) = json(&["estimate", "--input", s(&still), "--coverage", "measured"]);
    assert_eq!(code, 0, "{estimate}");
    assert_eq!(estimate["coverage_banner"]["code"], "brief_face");
    assert_eq!(estimate["can_start"], true);
    let (code, leave) = json(&["leave", "--url", "https://www.fbi.gov/wanted/x"]);
    assert_eq!((code, leave["message"]["code"].as_str()), (0, Some("leaving")));
    let (code, leave) = json(&["leave", "--url", "https://www.fbi.gov.example/x"]);
    assert_eq!((code, leave["refusal"].as_str()), (2, Some("only_fbi")));
    let out = root.path().join("r");
    assert_eq!(json(&["scan", "--input", s(&still), "--posters", s(&posters), "--out", s(&out)]).0, 0);
    assert_eq!(json(&["sweep", "--root", s(root.path())]).1["removed"], 0);
    let (code, deleted) = json(&["delete", "--out", s(&out)]);
    assert_eq!((code, deleted["headline"]["code"].as_str()), (0, Some("deleted")));
    assert!(!out.exists());
    let (code, refused) = json(&["delete", "--out", s(&posters)]);
    assert_eq!((code, refused["refusal"].as_str()), (2, Some("delete_foreign")));
    assert!(posters.join("snapshot.json").is_file());
    let (code, words) = json(&["copy"]);
    assert_eq!(code, 0);
    assert_eq!(words["copy"]["possible_candidate"], "Possible candidate. Not an identification.");
}

#[test]
fn the_self_test_holds_the_pipeline_rules() {
    let (code, result) = json(&["selftest"]);
    assert_eq!(code, 0, "{result}");
    let n = |key: &str| result[key].as_u64().unwrap();
    assert_eq!(n("impostor_candidates"), 0);
    assert!(n("genuine_candidates") > 0);
    assert_eq!(n("small_compared"), 0);
    assert_eq!(n("small_seen"), n("small_compared") + 10);
    assert_eq!(n("one_frame_video_candidates"), 0, "one video frame is too brief");
    assert_eq!(n("brief_complete_found"), 1);
    assert_eq!(n("second_complete_found"), 1);
    assert_eq!(n("second_measured_found"), 1);
}
