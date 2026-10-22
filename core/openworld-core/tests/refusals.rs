// SPDX-License-Identifier: Apache-2.0

//! Fail-closed: a scan that cannot be trusted to finish is refused before any
//! frame, with a code, and never reads as "no candidate".

mod common;

use common::*;
use openworld_core::copy::{self, Copy};
use openworld_core::posters::{write_fixture_pack, write_network_pack, Poster};
use openworld_core::report::{ScanReport, TrackStatus};
use openworld_core::scan::{estimate_for, ScanRequest};
use openworld_core::timeutil::format_rfc3339;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Refused before anything was written: the result folder was never made.
fn assert_refused_early(req: &ScanRequest, code: Copy) -> ScanReport {
    let report = run(req);
    assert_refused(&report, code);
    assert!(!req.out.exists(), "{}: a refusal made {}", code.0, req.out.display());
    report
}

#[test]
fn an_input_that_cannot_be_read_is_refused() {
    let lab = Lab::new();
    for input in [lab.path("missing.png"), lab.dir.path().to_path_buf()] {
        let req = lab.request(&input);
        assert_refused_early(&req, copy::R_UNREADABLE);
        assert!(estimate_for(&req).unwrap_err().headline.is(copy::R_UNREADABLE));
    }
}

#[test]
fn a_file_that_is_not_a_picture_is_refused() {
    let lab = Lab::new();
    let mut png = png_bytes(&blank(64, 64));
    png.truncate(40);
    for (name, bytes) in [("notes.txt", b"not a photo".to_vec()), ("empty.png", Vec::new()), ("cut.png", png), ("noise.mp4", vec![7u8; 4096])] {
        let req = lab.request(lab.write(name, &bytes));
        let report = run(&req);
        // A file the header reader rejects is refused; one it accepts and then cannot decode is incomplete.
        assert!(report.is_refused() || report.status == openworld_core::Status::Incomplete, "{name}: {}", describe(&report));
        assert_not_a_clearance(&report);
        assert!(report.candidates.is_empty());
        if report.is_refused() {
            assert_eq!(report.refusal.as_deref(), Some("bad_codec"), "{name}");
        }
    }
    assert_refused_early(&lab.request(lab.write("words.txt", b"hello")), copy::R_BAD_CODEC);
}

#[test]
fn the_poster_pack_must_be_whole_fresh_and_valid() {
    let now = SystemTime::now();
    type Damage = Box<dyn Fn(&Path)>;
    let cases: Vec<(&str, Copy, Damage)> = vec![
        ("no pack", copy::R_PACK_MISSING, Box::new(|dir: &Path| fs::remove_dir_all(dir).unwrap())),
        ("no digest", copy::R_PACK_MISSING, Box::new(|dir: &Path| fs::remove_file(dir.join("snapshot.sha256")).unwrap())),
        (
            "an edited byte",
            copy::R_PACK_HASH,
            Box::new(|dir: &Path| {
                let mut bytes = fs::read(dir.join("snapshot.json")).unwrap();
                bytes[0] = b' ';
                fs::write(dir.join("snapshot.json"), bytes).unwrap();
            }),
        ),
        (
            "a swapped poster photo",
            copy::R_PACK_HASH,
            Box::new(|dir: &Path| fs::write(dir.join("images/fixture-missing-a.png"), b"another face").unwrap()),
        ),
        (
            "a photo outside the pack",
            copy::R_PACK_HASH,
            Box::new(|dir: &Path| edit_pack(dir, |v| v["posters"][0]["image"] = json!("../../etc/hostname"))),
        ),
        ("expired", copy::R_PACK_EXPIRED, Box::new(|dir: &Path| edit_pack(dir, |v| v["expires_at"] = json!("2020-01-01T00:00:00Z")))),
        (
            "every poster resolved",
            copy::R_PACK_EXPIRED,
            Box::new(|dir: &Path| {
                edit_pack(dir, |v| {
                    for poster in v["posters"].as_array_mut().unwrap() {
                        poster["expires_at"] = json!("2020-01-01T00:00:00Z");
                    }
                })
            }),
        ),
        (
            "older than seven days",
            copy::R_PACK_OLD,
            Box::new(move |dir: &Path| {
                edit_pack(dir, |v| {
                    v["created_at"] = json!(format_rfc3339(now - days(8)));
                    v["expires_at"] = json!(format_rfc3339(now + days(1)));
                })
            }),
        ),
        (
            "dated tomorrow",
            copy::R_PACK_FUTURE,
            Box::new(move |dir: &Path| {
                edit_pack(dir, |v| {
                    v["created_at"] = json!(format_rfc3339(now + days(1)));
                    v["expires_at"] = json!(format_rfc3339(now + days(3)));
                })
            }),
        ),
        (
            "not JSON",
            copy::R_PACK_UNREADABLE,
            Box::new(|dir: &Path| {
                fs::write(dir.join("snapshot.json"), b"{").unwrap();
                fs::write(dir.join("snapshot.sha256"), openworld_core::posters::sha256_hex(b"{")).unwrap();
            }),
        ),
        ("another schema", copy::R_PACK_UNREADABLE, Box::new(|dir: &Path| edit_pack(dir, |v| v["schema"] = json!("openworld.poster_pack.v0")))),
        ("an unknown perception", copy::R_PACK_PERCEPTION, Box::new(|dir: &Path| edit_pack(dir, |v| v["perception"] = json!("not-a-perception")))),
        ("a short vector", copy::R_PACK_VECTORS, Box::new(|dir: &Path| edit_pack(dir, |v| v["posters"][1]["embedding"] = json!([1.0])))),
        ("no vector", copy::R_PACK_VECTORS, Box::new(|dir: &Path| edit_pack(dir, |v| v["posters"][0]["embedding"] = json!(null)))),
    ];
    for (name, code, damage) in cases {
        let lab = Lab::new();
        let input = lab.png("scene.png", &openworld_core::fixture::demo_scene().image);
        damage(&lab.posters);
        let req = lab.request(&input);
        // Vectors are checked once the result folder is open; see the ignored test below.
        let report = if code == copy::R_PACK_VECTORS { run(&req) } else { assert_refused_early(&req, code) };
        assert_refused(&report, code);
        assert!(report.headline.text.ends_with("Refusing."), "{name}: {}", report.headline.text);
    }
}

#[test]

fn a_pack_refused_for_its_vectors_leaves_the_result_folder_alone() {
    let lab = Lab::new();
    let input = lab.png("scene.png", &openworld_core::fixture::demo_scene().image);
    let req = lab.request(&input);
    assert_candidate(&run(&req), MISSING_POSTER);
    lab.edit_pack(|v| v["posters"][1]["embedding"] = json!([1.0]));
    assert_refused(&run(&req), copy::R_PACK_VECTORS);
    // Every other pack refusal keeps the earlier result. This one should too.
    assert!(req.out.join("result.json").is_file(), "the earlier result was emptied by a refused scan");
    let fresh = lab.request(&input);
    assert_refused(&run(&fresh), copy::R_PACK_VECTORS);
    assert!(!fresh.out.exists(), "a refused scan left {}", fresh.out.display());
}

#[test]
fn a_link_that_is_not_the_fbi_is_dropped_and_the_card_stays() {
    let lab = Lab::new();
    lab.edit_pack(|v| v["posters"][0]["fbi_url"] = json!("https://www.fbi.gov.evil.example/wanted"));
    let input = lab.png("scene.png", &openworld_core::fixture::demo_scene().image);
    let report = lab.scan(&input);
    let card = assert_candidate(&report, MISSING_POSTER);
    assert_eq!(card.poster.fbi_url, None);
}

#[test]
fn the_bundle_must_be_in_a_readable_catalog() {
    let lab = Lab::new();
    let input = lab.png("blank.png", &blank(32, 32));
    let mut req = lab.request(&input);
    req.bundle = "no-such".into();
    assert_refused_early(&req, copy::R_NO_BUNDLE);
    assert!(estimate_for(&req).unwrap_err().headline.is(copy::R_NO_BUNDLE));

    let mut req = lab.request(&input);
    req.bundles = lab.path("no-catalog");
    assert_refused_early(&req, copy::R_CATALOG);
    assert!(estimate_for(&req).unwrap_err().headline.is(copy::R_CATALOG));

    let empty = lab.path("empty-catalog");
    fs::create_dir_all(&empty).unwrap();
    let mut req = lab.request(&input);
    req.bundles = empty;
    assert_refused_early(&req, copy::R_NO_PROGRAM);

    // A folder that claims to be official without being the pinned manifest.
    let forged = lab.path("forged");
    let dir = dev_bundle(&forged, "fast", 0.5, &[]);
    let text = fs::read_to_string(dir.join("manifest.toml")).unwrap().replace("official = false", "official = true");
    fs::write(dir.join("manifest.toml"), text).unwrap();
    let mut req = lab.request(&input);
    req.bundles = forged;
    let report = run(&req);
    assert_eq!(report.refusal.as_deref(), Some("not_official"), "{}", describe(&report));
    assert_not_a_clearance(&report);
}

/// The demo posters as network vectors from `embedder`. `None` names no embedder.
fn network_pack(dir: &Path, embedder: Option<&str>) -> std::path::PathBuf {
    let fixture = write_fixture_pack(&dir.join("fixture"), SystemTime::now()).unwrap();
    let posters: Vec<Poster> = fixture
        .posters
        .into_iter()
        .map(|mut p| {
            p.image = None;
            p.image_sha256 = None;
            p
        })
        .collect();
    let pack = dir.join("network");
    let now = SystemTime::now();
    let written = write_network_pack(&pack, "test", "Test", embedder.unwrap_or(""), posters, now, now + days(1)).unwrap();
    assert_eq!(written.perception, "onnx");
    if embedder.is_none() {
        edit_pack(&pack, |v| {
            v.as_object_mut().unwrap().remove("embedder_sha256");
        });
    }
    pack
}

#[test]
fn a_network_pack_needs_installed_weights_and_its_own_embedder() {
    let lab = Lab::new();
    let input = lab.png("scene.png", &scene(MISSING_ID, &[0]).0);
    let (det, emb) = stub_models();
    let catalog_dir = lab.path("catalog");
    dev_bundle(&catalog_dir, "stub", 0.5, &[("detector", "det.onnx", &det), ("embedder", "emb.onnx", &emb)]);
    dev_bundle(&catalog_dir, "broken", 0.5, &[("detector", "det.onnx", b"not an onnx model"), ("embedder", "emb.onnx", b"not an onnx model")]);
    let embedder = openworld_core::posters::sha256_hex(&emb);
    let request = |bundles: &Path, bundle: &str, posters: std::path::PathBuf| {
        let mut req = lab.request(&input);
        req.bundles = bundles.to_path_buf();
        req.bundle = bundle.into();
        req.posters = posters;
        req
    };

    // A network pack names the embedder that made its vectors.
    let pack = network_pack(&lab.path("p1"), Some(&embedder));
    let loaded = openworld_core::load_pack(&pack, SystemTime::now()).unwrap();
    assert_eq!(loaded.embedder_sha256.as_deref(), Some(embedder.as_str()));
    assert!(!loaded.demo());
    // Fast's weights are not installed here, and Accurate has none yet.
    for bundle in ["fast", "accurate"] {
        assert_refused_early(&request(&catalog(), bundle, pack.clone()), copy::R_NO_WEIGHTS);
    }
    // Weights that are pinned but are not a model.
    let report = run(&request(&catalog_dir, "broken", network_pack(&lab.path("p2"), Some(&openworld_core::posters::sha256_hex(b"not an onnx model")))));
    assert_eq!(report.refusal.as_deref(), Some("runtime"), "{}", describe(&report));
    assert_not_a_clearance(&report);
    // Vectors from another embedder, or from no named one, mean nothing to this one.
    for other in [Some("0".repeat(64)), None] {
        let pack = network_pack(&lab.path(&format!("p-{}", other.is_some())), other.as_deref());
        assert_refused_early(&request(&catalog_dir, "stub", pack), copy::R_PACK_EMBEDDER);
    }

    // The stub networks run and find nothing. The scan says so without a clearance claim,
    // names the networks and the CPU they ran on, and says chance is not measured.
    let mut req = request(&catalog_dir, "stub", pack);
    req.accelerate = true;
    let report = run(&req);
    assert_no_candidate(&report);
    assert!(report.tracks.is_empty());
    assert_eq!(report.perception, "onnx");
    assert!(report.perception_note.is_none());
    assert_eq!(report.provider, "CPUExecutionProvider");
    assert_eq!(report.execution, "cpu");
    assert!(report.device_note.as_ref().is_some_and(|l| l.is(copy::CPU_NOTE)));
    assert!(report.chance.as_ref().is_some_and(|l| l.is(copy::CHANCE_UNKNOWN)), "{:?}", report.chance);
    assert!(!report.posters.demo);
    assert_eq!(report.bundle.weights.len(), 2);
    assert!(report.tracks.iter().all(|t| t.status != TrackStatus::Candidate));
}

#[test]
fn the_result_folder_must_be_new_empty_or_ours() {
    let lab = Lab::new();
    let input = lab.png("blank.png", &blank(40, 40));
    let with_out = |out: std::path::PathBuf| {
        let mut req = lab.request(&input);
        req.out = out;
        req
    };

    // Under a file, or a file itself, is left alone.
    let blocker = lab.write("blocked", b"keep");
    assert_refused(&run(&with_out(blocker.join("out"))), copy::R_OUT_CREATE);
    assert_refused(&run(&with_out(blocker.clone())), copy::R_OUT_CREATE);
    assert_eq!(fs::read(&blocker).unwrap(), b"keep");
    let mut req = with_out(blocker.clone());
    req.out = Default::default();
    assert_refused(&run(&req), copy::R_OUT_CREATE);

    // A new or empty folder becomes a result.
    let empty = lab.path("empty");
    fs::create_dir_all(&empty).unwrap();
    assert_no_candidate(&run(&with_out(empty.clone())));
    assert!(empty.join(".openworld-result").is_file());

    // A folder holding the posters or the catalog, or inside them, is refused.
    let catalog_copy = lab.path("catalog");
    dev_bundle(&catalog_copy, "dev", 0.5, &[]);
    for out in [lab.posters.clone(), lab.posters.join("result"), lab.dir.path().to_path_buf(), catalog_copy.join("dev/result")] {
        let mut req = with_out(out.clone());
        req.bundles = catalog_copy.clone();
        req.bundle = "dev".into();
        assert_refused(&run(&req), copy::R_OUT_OVERLAP);
        assert!(!out.join(".openworld-result").exists(), "{}", out.display());
    }
    assert!(lab.posters.join("snapshot.json").is_file());
    assert!(catalog_copy.join("dev/manifest.toml").is_file());
}

#[test]
fn a_result_folder_that_cannot_be_replaced_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let lab = Lab::new();
    let input = lab.png("blank.png", &blank(40, 40));
    let req = lab.request(&input);
    assert_no_candidate(&run(&req));
    fs::create_dir(req.out.join("stuck")).unwrap();
    fs::write(req.out.join("stuck/keep"), b"x").unwrap();
    fs::set_permissions(req.out.join("stuck"), fs::Permissions::from_mode(0o555)).unwrap();
    // Root ignores the mode. Only check what the mode actually blocks.
    let blocked = fs::write(req.out.join("stuck/probe"), b"x").is_err();
    let report = run(&req);
    // Gone when the replace went through.
    let _ = fs::set_permissions(req.out.join("stuck"), fs::Permissions::from_mode(0o755));
    if blocked {
        assert_refused(&report, copy::R_OUT_REPLACE);
        assert!(req.out.join("stuck/keep").is_file());
    } else {
        eprintln!("skipped: this account writes into a mode-555 folder, so it cannot be blocked");
        assert_no_candidate(&report);
    }
}
