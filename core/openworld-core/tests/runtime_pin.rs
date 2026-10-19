// SPDX-License-Identifier: Apache-2.0

//! ONNX Runtime is loaded only when its bytes are the pinned build. This file is
//! its own test program, so the runtime it loads (once per process) is the one
//! `OPENWORLD_ORT_LIB` names here, and no other test can load the real one first.

mod common;

use common::*;
use openworld_core::posters::{write_fixture_pack, write_network_pack, Poster};
use std::time::SystemTime;

#[test]
fn a_library_that_is_not_the_pinned_build_is_a_runtime_refusal() {
    let lab = Lab::new();
    // Any bytes but the pinned ones. A real, loadable ONNX Runtime of another build is refused
    // the same way, before it is loaded.
    let impostor = lab.write("libonnxruntime-impostor", b"not the pinned ONNX Runtime");
    std::env::set_var("OPENWORLD_ORT_LIB", &impostor);

    let input = lab.png("scene.png", &scene(MISSING_ID, &[0]).0);
    let (det, emb) = stub_models();
    let catalog_dir = lab.path("catalog");
    dev_bundle(&catalog_dir, "stub", 0.5, &[("detector", "det.onnx", &det), ("embedder", "emb.onnx", &emb)]);
    let fixture = write_fixture_pack(&lab.path("fixture"), SystemTime::now()).unwrap();
    let posters: Vec<Poster> = fixture
        .posters
        .into_iter()
        .map(|mut p| {
            p.image = None;
            p.image_sha256 = None;
            p
        })
        .collect();
    let pack = lab.path("network");
    let now = SystemTime::now();
    write_network_pack(&pack, "test", "Test", &openworld_core::posters::sha256_hex(&emb), posters, now, now + days(1)).unwrap();

    let mut req = lab.request(&input);
    req.bundles = catalog_dir;
    req.bundle = "stub".into();
    req.posters = pack;
    let report = run(&req);
    // The existing runtime refusal: shells read the code, and the text says why.
    assert_refused(&report, openworld_core::copy::R_RUNTIME);
    if openworld_core::runtime::pinned_library().is_some() {
        assert!(report.headline.text.contains("not the pinned build"), "{}", describe(&report));
    }
    assert!(!openworld_core::runtime::linked());
}
