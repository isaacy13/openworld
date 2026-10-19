// SPDX-License-Identifier: Apache-2.0

//! Pins the official bundle manifests into the library, builds in the one
//! ONNX Runtime library digest this target may load (from `onnxruntime.pins`),
//! and names the copy `scripts/fetch_ort.py` placed for development.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest_dir.join("../..");
    pin_official_bundles(&repo.join("bundles"));
    pin_runtime(&manifest_dir.join("onnxruntime.pins"), &repo.join("third_party/onnxruntime"));
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A manifest that says `official = true` is official only when its bytes are
/// the ones in this tree when the library was built. Its curve is pinned too.
fn pin_official_bundles(root: &Path) {
    println!("cargo:rerun-if-changed={}", root.display());
    let mut rows = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.join("manifest.toml").is_file()).collect();
        dirs.sort();
        for dir in dirs {
            let manifest = dir.join("manifest.toml");
            println!("cargo:rerun-if-changed={}", manifest.display());
            let bytes = fs::read(&manifest).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            if !text.lines().any(|line| line.trim().replace(' ', "") == "official=true") {
                continue;
            }
            let id = dir.file_name().unwrap().to_string_lossy().to_string();
            let curve = dir.join("curve.json");
            println!("cargo:rerun-if-changed={}", curve.display());
            let curve_sha = fs::read(&curve).ok().map(|b| format!("Some(\"{}\")", sha256_hex(&b))).unwrap_or_else(|| "None".into());
            rows.push(format!("    (\"{id}\", \"{}\", {curve_sha}),\n", sha256_hex(&bytes)));
        }
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("official_bundles.rs");
    let body = format!(
        "/// (folder id, manifest SHA-256, curve SHA-256) for each official bundle in this build.\npub const OFFICIAL_BUNDLES: &[(&str, &str, Option<&str>)] = &[\n{}];\n",
        rows.concat()
    );
    fs::write(out, body).unwrap();
}

/// The ONNX Runtime pin for the target, from `onnxruntime.pins` (the file
/// `scripts/fetch_ort.py` reads too), and the copy that script placed for development.
fn pin_runtime(pins: &Path, root: &Path) {
    println!("cargo:rerun-if-changed={}", pins.display());
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_DIRECTML");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let directml = std::env::var_os("CARGO_FEATURE_DIRECTML").is_some();
    let platform = match (os.as_str(), arch.as_str()) {
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("linux", "aarch64") => Some("linux-aarch64"),
        ("windows", "x86_64") if directml => Some("windows-x86_64-directml"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("macos", "aarch64") => Some("macos-arm64"),
        // Android loads the library from its signed APK; iOS links it statically.
        _ => None,
    };
    let text = fs::read_to_string(pins).unwrap_or_else(|err| panic!("{}: {err}", pins.display()));
    let library = platform.map(|platform| library_pin(&text, platform).unwrap_or_else(|| panic!("onnxruntime.pins has no library line for {platform}")));
    let quote = |v: Option<&str>| v.map(|v| format!("Some(\"{v}\")")).unwrap_or_else(|| "None".into());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("ort_pin.rs");
    let body = format!(
        "/// The `onnxruntime.pins` platform this build loads ONNX Runtime for, if any.\npub const ORT_PLATFORM: Option<&str> = {};\n/// The only SHA-256 of the ONNX Runtime library this build loads.\npub const ORT_LIBRARY_SHA256: Option<&str> = {};\n",
        quote(platform),
        quote(library.as_ref().map(|(sha, _)| sha.as_str())),
    );
    fs::write(out, body).unwrap();
    if let (Some(platform), Some((_, leaf))) = (platform, library) {
        let path = root.join(platform).join(leaf);
        println!("cargo:rerun-if-changed={}", path.display());
        if let Ok(path) = path.canonicalize() {
            println!("cargo:rustc-env=OPENWORLD_ORT_DEV={}", path.display());
        }
    }
}

/// (sha256, installed file name) of the `library` line for `platform`.
fn library_pin(text: &str, platform: &str) -> Option<(String, String)> {
    let mut found = None;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() == 5 && fields[0] == platform && fields[1] == "library" {
            let sha = fields[2];
            assert!(sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "onnxruntime.pins: {sha} is not a lowercase SHA-256");
            assert!(found.is_none(), "onnxruntime.pins has two library lines for {platform}");
            found = Some((sha.to_string(), fields[3].to_string()));
        }
    }
    found
}
