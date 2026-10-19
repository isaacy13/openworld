// SPDX-License-Identifier: Apache-2.0

//! What the shells read before Analyze: the bundle catalog, the estimate, the
//! typed JSON calls, and the sentences by code. And the demo self-test.

mod common;

use common::*;
use openworld_core::bundle::{load_bundle, load_bundles, row_json};
use openworld_core::copy::{self, class_note, estimate_class_line, product_copy};
use openworld_core::estimate::Coverage;
use openworld_core::invoke;
use openworld_core::measure::self_test;
use openworld_core::posters::{write_fixture_pack, PosterClass};
use openworld_core::scan::estimate_for;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

fn ask(call: Value) -> Value {
    invoke::call(&call.to_string(), None, &mut |_| {})
}

#[test]
fn the_catalog_lists_fast_first_and_keeps_accurate_unmeasured() {
    let all = load_bundles(&catalog()).unwrap();
    let ids: Vec<&str> = all.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids, ["fast", "accurate"]);
    let (fast, accurate) = (&all[0], &all[1]);
    assert!(fast.preselected() && !accurate.preselected());
    assert!(fast.official && accurate.official);
    assert_eq!((fast.detector.as_str(), fast.embedder.as_str()), ("YuNet", "SFace"));
    assert_eq!(fast.best_for, "Phones and long video.");
    // The weights are not in the tree; the manifest pins them for the download.
    assert!(!fast.weights_ready);
    assert!(fast.weights.iter().all(|w| w.sha256.len() == 64 && !w.present));
    assert!(fast.threshold > 0.0 && fast.threshold < 1.0);
    assert_eq!(accurate.best_for, "A computer, when you want fewer misses.");
    assert!(!accurate.best_for.to_ascii_lowercase().contains("more accurate"));
    assert!(!accurate.weights_ready);
    assert!(!accurate.curve_exists());
    assert!(accurate.curve_line().is(copy::NOT_MEASURED));
    assert!(all.iter().all(|b| !b.real_posters_allowed()), "real posters stay off");
    assert_ne!(fast.threshold, accurate.threshold, "each bundle has its own locked cutoff");

    let rows = ask(json!({"v": 1, "op": "bundles", "bundles": catalog()}));
    assert_eq!(rows["ok"], true);
    assert_eq!(rows["bundles"][0]["id"], "fast");
    assert_eq!(rows["bundles"][0]["preselected"], true);
    assert_eq!(rows["bundles"][1]["curve_line"]["code"], "not_measured");
    assert_eq!(rows["note"]["code"], "bundle_note");
    assert!(rows["bundles"].as_array().unwrap().iter().all(|r| r["real_posters_allowed"] == false));
    assert_eq!(ask(json!({"v": 1, "op": "posters_update", "bundles": catalog()}))["refusal"], "real_posters_off");
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn a_bundle_folder_copied_on_its_own_is_still_the_official_one() {
    // An install copies bundle folders. Trust comes from the pinned bytes, not the path.
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&catalog().join("fast"), &dir.path().join("fast"));
    let copied = load_bundles(dir.path()).unwrap();
    assert_eq!(copied.len(), 1);
    assert!(copied[0].official);
    assert_eq!(copied[0].curve_exists(), bundle("fast").curve_exists());
    // One changed byte and it is no longer official.
    let manifest = dir.path().join("fast/manifest.toml");
    let text = fs::read_to_string(&manifest).unwrap().replace("Phones and long video.", "Phones and long videos.");
    fs::write(&manifest, text).unwrap();
    assert_eq!(load_bundles(dir.path()).unwrap_err().code, "not_official");
}

#[test]
fn a_developer_manifest_must_say_it_is_unofficial() {
    let root = tempfile::tempdir().unwrap();
    let dir = dev_bundle(root.path(), "dev", 0.5, &[]);
    let bundle = load_bundle(&dir).unwrap();
    assert!(!bundle.official);
    assert!(bundle.unofficial_notice.to_ascii_lowercase().contains("unofficial"));
    assert!(!bundle.real_posters_allowed());
    let row = row_json(&bundle);
    assert_eq!(row["official"], false);
    assert_eq!(row["unofficial_notice"], "Unofficial developer bundle.");

    let manifest = dir.join("manifest.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    for (from, to) in [
        ("unofficial = true\n", ""),
        ("unofficial_notice = \"Unofficial developer bundle.\"", "unofficial_notice = \"A developer bundle.\""),
        ("unofficial_notice = \"Unofficial developer bundle.\"\n", ""),
    ] {
        fs::write(&manifest, original.replace(from, to)).unwrap();
        assert_eq!(load_bundle(&dir).unwrap_err().code, "unofficial_unstated", "{from:?} -> {to:?}");
    }
    fs::write(&manifest, original.replace("estimate_factor = 1.0", "estimate_factor = 1.0\nsurprise = 1")).unwrap();
    assert!(load_bundle(&dir).unwrap_err().is(copy::R_CATALOG), "an unknown manifest field is refused");
    fs::write(&manifest, "schema = \"nope\"\n").unwrap();
    assert!(load_bundle(&dir).is_err());
}

#[test]
fn the_estimate_follows_the_file_the_coverage_and_the_pack() {
    let lab = Lab::new();
    let still = lab.png("still.png", &blank(80, 60));
    let photo = estimate_for(&lab.request(&still)).unwrap();
    assert_eq!(photo.frames_analyzed, 1);
    assert_eq!(photo.human.text, "Less than a second");
    assert!(photo.timed);
    assert!(photo.caveat.is(copy::ESTIMATE_CAVEAT));
    assert!(photo.notes.iter().any(|n| n.is(copy::CPU_NOTE)), "{:?}", photo.notes);
    assert!(photo.coverage_banner.is_none());

    // Demo markers are read at the original size; the estimate says so.
    let large = lab.write("large.tif", &tiff(&[TiffPage::rgb(&blank(4032, 3024), 1)]));
    let planned = estimate_for(&lab.request(&large)).unwrap();
    assert_eq!(planned.detection_long_side, 4032);
    assert_eq!(planned.detection_line.code, "detect_full");
    // Without a pack to read, the plan uses the bundle's detector size, and it is rough.
    let mut req = lab.request(&large);
    req.posters = lab.path("no-pack");
    let rough = estimate_for(&req).unwrap();
    assert!(rough.detection_long_side < 4032 && rough.detection_long_side >= 640, "{}", rough.detection_long_side);
    assert_eq!(rough.detection_line.code, "detect_scaled");
    assert!(!rough.timed);
    assert!(rough.notes.iter().any(|n| n.is(copy::ESTIMATE_ROUGH)));

    // A video: every frame, or five a second.
    let video = lab.path("clip.mkv");
    encode(lab.dir.path(), &(0..30).map(plain).collect::<Vec<_>>(), 30, FFV1, &video);
    let mut req = lab.request(&video);
    assert_eq!(estimate_for(&req).unwrap().frames_analyzed, 30);
    req.coverage = Coverage::Measured;
    let measured = estimate_for(&req).unwrap();
    assert_eq!(measured.frames_analyzed, 5);
    assert!(measured.coverage_banner.as_ref().is_some_and(|l| l.is(copy::BRIEF_FACE)));
    assert!(measured.coverage_line.is(copy::COVERAGE_MEASURED));
    // The estimate is what the scan then reads.
    let report = run(&req);
    assert_eq!(report.counts.frames_analyzed, measured.frames_analyzed);

    // The typed call adds the class line and whether Analyze can start.
    let answer = ask(json!({"v": 1, "op": "estimate", "input": still, "bundles": catalog(), "posters": lab.posters, "missing": false, "wanted": false}));
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["classes"]["code"], "choose_class");
    assert_eq!(answer["can_start"], false);
    let answer = ask(json!({"v": 1, "op": "estimate", "input": lab.path("gone.png"), "bundles": catalog(), "posters": lab.posters}));
    assert_eq!(answer["refusal"], "unreadable");
}

#[test]
fn the_typed_entry_refuses_a_broken_request() {
    for text in ["{", "[]", "{\"op\":\"copy\"}", "{\"v\":2,\"op\":\"copy\"}", "{\"v\":1,\"op\":\"dance\"}", "{\"v\":1,\"op\":\"scan\",\"input\":1}"] {
        let value = invoke::call(text, None, &mut |_| {});
        assert_eq!(value["ok"], false, "{text}");
        assert_eq!(value["status"], "refused", "{text}");
        assert_ne!(value["headline"]["code"], "no_clearance", "{text}");
    }
    // A misspelled option is refused, never ignored.
    let lab = Lab::new();
    let scan = ask(json!({"v": 1, "op": "scan", "input": lab.path("x.png"), "bundles": catalog(), "posters": lab.posters, "out": lab.out(), "coverag": "measured"}));
    assert_eq!(scan["refusal"], "bad_request");
    let version = ask(json!({"v": 1, "op": "version"}));
    assert_eq!(version["api"], invoke::API_VERSION);
    assert_eq!(version["demo_posters"], true);
    assert!(version["providers"].as_array().unwrap().iter().any(|p| p == "CPUExecutionProvider"));
    let leave = ask(json!({"v": 1, "op": "leave", "url": "https://www.fbi.gov/wanted/topten"}));
    assert_eq!(leave["message"]["code"], "leaving");
    assert_eq!(ask(json!({"v": 1, "op": "leave", "url": "https://fbi.gov.example/"}))["refusal"], "only_fbi");

    // Posters: write the demo pack, then check it.
    let out = lab.path("demo-pack");
    let demo = ask(json!({"v": 1, "op": "posters_demo", "out": out}));
    assert_eq!(demo["ok"], true);
    assert_eq!(demo["posters"]["demo"], true);
    let checked = ask(json!({"v": 1, "op": "posters_check", "posters": out}));
    assert_eq!(checked["posters"]["missing"], 1);
    assert_eq!(checked["posters"]["wanted"], 1);
    assert_eq!(ask(json!({"v": 1, "op": "posters_check", "posters": lab.path("none")}))["refusal"], "missing_pack");
    assert_eq!(ask(json!({"v": 1, "op": "delete", "out": lab.dir.path()}))["refusal"], "delete_foreign");
}

#[test]
fn every_sentence_has_a_code_and_the_class_lines_read_naturally() {
    let table = product_copy();
    for key in [
        "possible_candidate",
        "not_compared",
        "too_brief",
        "incomplete",
        "no_clearance",
        "leaving",
        "not_sending",
        "leave_wanted",
        "leave_missing",
        "unauthenticated",
        "why_small",
        "stopped_heat",
        "not_fully_decoded",
        "too_large",
        "bad_vectors",
        "out_foreign",
        "out_overlap",
    ] {
        assert!(table[key].as_str().is_some_and(|t| t.len() > 3), "{key}");
    }
    assert_eq!(table["no_camera"], "Import a file you already have. There is no camera.");
    assert_eq!(table["no_class"], "No class was on.");
    assert_eq!(class_note(false, true).1, "Wanted.");
    assert_eq!(class_note(true, false).1, "Missing.");
    assert_eq!(estimate_class_line(true, true).1, "Missing and wanted.");
    assert_eq!(estimate_class_line(false, false), copy::CHOOSE_CLASS);
    assert_eq!(PosterClass::Missing.label(), "Missing");
    assert_eq!(PosterClass::Wanted.label(), "Wanted");
    assert!(Coverage::parse("sometimes").is_none());
    assert_eq!(Coverage::parse("measured"), Some(Coverage::Measured));
    // Refusals say so.
    for (code, text) in table.as_object().unwrap() {
        if ["unreadable", "bad_codec", "too_large", "missing_pack", "bad_hash", "expired_pack", "bad_vectors", "out_foreign"].contains(&code.as_str()) {
            assert!(text.as_str().unwrap().ends_with("Refusing."), "{code}");
        }
    }
}

#[test]
fn the_demo_self_test_keeps_its_rules() {
    let dir = tempfile::tempdir().unwrap();
    let pack = write_fixture_pack(dir.path(), SystemTime::now()).unwrap();
    let result = self_test(&bundle("fast"), &pack);
    assert_eq!(result.cutoff, openworld_core::fixture::FIXTURE_CUTOFF);
    assert_eq!(result.genuine_trials, 40);
    assert!(result.genuine_candidates > 0 && result.genuine_candidates <= result.genuine_trials);
    assert_eq!(result.impostor_trials, 40);
    assert_eq!(result.impostor_candidates, 0, "a marker for no poster is never a candidate");
    assert_eq!(result.small_seen, 10);
    assert_eq!(result.small_compared, 0, "a 64 px marker is never compared");
    assert_eq!(result.one_frame_video_candidates, 0);
    assert_eq!(result.brief_complete_found, 1);
    assert_eq!(result.second_complete_found, 1);
}
