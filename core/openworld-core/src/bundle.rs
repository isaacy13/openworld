// SPDX-License-Identifier: Apache-2.0

//! A bundle is a folder: manifest, ONNX weights, a locked threshold, and a
//! one-line best-for. Add or remove a model by adding or removing a folder.
//!
//! Trust does not come from the folder vouching for itself:
//! - `official = true` holds only for a manifest whose bytes, and whose curve,
//!   are the ones this library was built with. Anything else must say it is
//!   unofficial.
//! - Each folder is named after its id, and an id appears once.
//! - Weight paths stay inside the folder. Each weight is pinned by SHA-256
//!   and license, and the bytes that were hashed are the bytes loaded.
//! - The threshold is a number between 0 and 1.

use crate::copy::{self, Line};
use crate::posters::sha256_hex;
use serde::Deserialize;
use std::fs;
use std::path::{Component, Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/official_bundles.rs"));

pub const FAST_ID: &str = "fast";

#[derive(Clone, Debug)]
pub struct WeightPin {
    pub role: String,
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub license: String,
    pub path: String,
    pub present: bool,
}

/// What a bundle's measured curve says, when it has one.
#[derive(Clone, Debug, Default)]
pub struct Curve {
    pub picker_line: String,
    pub real_posters_allowed: bool,
    /// False match rate at the locked cutoff, and what it was measured on.
    pub fmr: Option<f64>,
    pub fmr_source: String,
}

#[derive(Clone, Debug)]
pub struct Bundle {
    pub id: String,
    pub name: String,
    pub version: String,
    pub official: bool,
    pub unofficial_notice: String,
    pub best_for: String,
    pub threshold: f32,
    pub estimate_factor: f64,
    pub detector: String,
    pub detector_version: String,
    pub embedder: String,
    pub embedder_version: String,
    pub weights: Vec<WeightPin>,
    pub weights_ready: bool,
    pub curve: Option<Curve>,
    pub dir: PathBuf,
}

impl Bundle {
    pub fn preselected(&self) -> bool {
        self.id == FAST_ID
    }

    pub fn curve_exists(&self) -> bool {
        self.curve.is_some()
    }

    pub fn real_posters_allowed(&self) -> bool {
        self.official && self.curve.as_ref().is_some_and(|c| c.real_posters_allowed)
    }

    pub fn curve_line(&self) -> Line {
        match &self.curve {
            Some(curve) => Line::with("curve_measured", curve.picker_line.clone(), &[]),
            None => Line::of(copy::NOT_MEASURED),
        }
    }

    pub fn weight(&self, role: &str) -> Option<&WeightPin> {
        self.weights.iter().find(|w| w.role == role)
    }

    /// The weight's bytes, read once and checked against its pin. These are the
    /// bytes ONNX Runtime loads, so a file swapped after the check is not used.
    pub fn verified_bytes(&self, role: &str) -> Result<Vec<u8>, Line> {
        let pin = self.weights.iter().find(|w| w.role == role && w.present).ok_or_else(|| Line::of(copy::R_NO_WEIGHTS))?;
        let bytes = fs::read(self.dir.join(&pin.path)).map_err(|_| Line::of(copy::R_NO_WEIGHTS))?;
        if sha256_hex(&bytes) != pin.sha256.trim() {
            return Err(bundle_line("weight_hash", &self.id, "The bundle {id} weight hash does not match. Refusing."));
        }
        Ok(bytes)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    id: String,
    name: String,
    version: String,
    official: bool,
    #[serde(default)]
    unofficial: bool,
    #[serde(default)]
    unofficial_notice: String,
    best_for: String,
    threshold: f32,
    estimate_factor: f64,
    #[serde(default)]
    curve: String,
    models: Models,
    #[serde(default)]
    files: Vec<WeightFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Models {
    detector: String,
    detector_version: String,
    embedder: String,
    embedder_version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WeightFile {
    role: String,
    name: String,
    version: String,
    #[serde(default)]
    sha256: String,
    license: String,
    /// Where `scripts/fetch_weights.py` downloads the file. The library never fetches it.
    #[serde(default)]
    #[allow(dead_code)]
    url: String,
    path: String,
}

fn bundle_line(code: &str, id: &str, template: &str) -> Line {
    Line::with(code, template.replace("{id}", id), &[("bundle", id.into())])
}

/// Every bundle folder under `dir`, Fast first, then by id.
pub fn load_bundles(dir: &Path) -> Result<Vec<Bundle>, Line> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir).map_err(|_| Line::of(copy::R_CATALOG))? {
        let path = entry.map_err(|_| Line::of(copy::R_CATALOG))?.path();
        if path.join("manifest.toml").is_file() {
            dirs.push(path);
        }
    }
    dirs.sort();
    let mut bundles: Vec<Bundle> = Vec::new();
    for path in dirs {
        let bundle = load_bundle(&path)?;
        if bundles.iter().any(|b| b.id == bundle.id) {
            return Err(bundle_line("duplicate_bundle", &bundle.id, "Two bundle folders say they are {id}. Refusing."));
        }
        bundles.push(bundle);
    }
    // The bundle that will run is the first row. The rest stay in id order.
    bundles.sort_by_key(|bundle| !bundle.preselected());
    Ok(bundles)
}

pub fn load_bundle(dir: &Path) -> Result<Bundle, Line> {
    let manifest_path = dir.join("manifest.toml");
    let bytes = fs::read(&manifest_path).map_err(|_| Line::of(copy::R_CATALOG))?;
    let text = String::from_utf8(bytes.clone()).map_err(|_| Line::of(copy::R_CATALOG))?;
    let manifest: Manifest = toml::from_str(&text).map_err(|_| Line::of(copy::R_CATALOG))?;
    let id = manifest.id.clone();
    if manifest.schema != "openworld.bundle.v1" {
        return Err(Line::of(copy::R_CATALOG));
    }
    let folder = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if folder != id || id.is_empty() || !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(bundle_line("bundle_folder", &id, "The bundle {id} is not in a folder of that name. Refusing."));
    }
    if !(manifest.threshold > 0.0 && manifest.threshold < 1.0) || !(manifest.estimate_factor > 0.0) {
        return Err(bundle_line("bundle_threshold", &id, "The bundle {id} threshold is not between 0 and 1. Refusing."));
    }
    let curve_bytes = curve_bytes(dir, &manifest.curve);
    if manifest.official && !pinned_official(&id, &bytes, curve_bytes.as_deref()) {
        return Err(bundle_line(
            "not_official",
            &id,
            "The bundle {id} says it is official, but it is not the manifest this build trusts. Refusing.",
        ));
    }
    if !manifest.official && (!manifest.unofficial || !manifest.unofficial_notice.to_ascii_lowercase().contains("unofficial")) {
        return Err(bundle_line("unofficial_unstated", &id, "Bundle {id} is unofficial and the manifest does not say so. Refusing."));
    }
    let mut weights = Vec::new();
    for file in &manifest.files {
        if !inside(&file.path) {
            return Err(bundle_line("weight_path", &id, "A bundle {id} weight is outside its folder. Refusing."));
        }
        let path = dir.join(&file.path);
        let present = path.is_file();
        if present {
            if license_forbids_shipping(&file.license) {
                return Err(bundle_line("research_only", &id, "The bundle {id} weight license does not allow shipping it. Refusing."));
            }
            if file.sha256.trim().len() != 64 {
                return Err(bundle_line("unpinned_weight", &id, "Bundle {id} has a weight file with no pinned SHA-256. Refusing."));
            }
        }
        weights.push(WeightPin {
            role: file.role.clone(),
            name: file.name.clone(),
            version: file.version.clone(),
            sha256: file.sha256.trim().to_string(),
            license: file.license.clone(),
            path: file.path.clone(),
            present,
        });
    }
    let has = |role: &str| weights.iter().any(|w| w.role == role && w.present);
    let weights_ready = has("detector") && has("embedder");
    Ok(Bundle {
        id,
        name: manifest.name,
        version: manifest.version,
        official: manifest.official,
        unofficial_notice: manifest.unofficial_notice,
        best_for: manifest.best_for,
        threshold: manifest.threshold,
        estimate_factor: manifest.estimate_factor,
        detector: manifest.models.detector,
        detector_version: manifest.models.detector_version,
        embedder: manifest.models.embedder,
        embedder_version: manifest.models.embedder_version,
        weights,
        weights_ready,
        curve: curve_bytes.as_deref().and_then(read_curve),
        dir: dir.to_path_buf(),
    })
}

/// A relative path made only of names: no `..`, no root, no drive.
fn inside(path: &str) -> bool {
    !path.is_empty() && Path::new(path).components().all(|c| matches!(c, Component::Normal(_)))
}

fn pinned_official(id: &str, manifest: &[u8], curve: Option<&[u8]>) -> bool {
    OFFICIAL_BUNDLES.iter().any(|(pin_id, manifest_sha, curve_sha)| {
        *pin_id == id && *manifest_sha == sha256_hex(manifest) && curve.map(sha256_hex).as_deref() == *curve_sha
    })
}

/// The curve travels with its bundle folder. A path outside it is not read.
fn curve_bytes(dir: &Path, curve: &str) -> Option<Vec<u8>> {
    if !inside(curve) {
        return None;
    }
    fs::read(dir.join(curve)).ok()
}

fn read_curve(bytes: &[u8]) -> Option<Curve> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if value.get("schema").and_then(|v| v.as_str()) != Some("openworld.curve.v1") {
        return None;
    }
    let chance = value.get("chance");
    Some(Curve {
        picker_line: value.get("picker_line").and_then(|v| v.as_str()).unwrap_or("Curve file present.").to_string(),
        real_posters_allowed: value.get("real_posters_allowed").and_then(|v| v.as_bool()).unwrap_or(false),
        fmr: chance.and_then(|c| c.get("fmr_at_cutoff")).and_then(|v| v.as_f64()).filter(|f| f.is_finite() && *f >= 0.0),
        fmr_source: chance.and_then(|c| c.get("source")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
    })
}

pub fn row_json(bundle: &Bundle) -> serde_json::Value {
    serde_json::json!({
        "id": bundle.id,
        "name": bundle.name,
        "version": bundle.version,
        "best_for": bundle.best_for,
        "preselected": bundle.preselected(),
        "curve_exists": bundle.curve_exists(),
        "curve_line": bundle.curve_line(),
        "real_posters_allowed": bundle.real_posters_allowed(),
        "weights_ready": bundle.weights_ready,
        "official": bundle.official,
        "unofficial_notice": (!bundle.official).then_some(&bundle.unofficial_notice),
        "detector": bundle.detector,
        "embedder": bundle.embedder,
    })
}

/// InsightFace zoo files say non-commercial research only. Those bytes are not an official bundle.
pub fn license_forbids_shipping(license: &str) -> bool {
    let text = license.to_ascii_lowercase();
    text.contains("non-commercial") || text.contains("noncommercial") || text.contains("research only") || text.contains("research-only")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A developer bundle in `root/<id>`, labeled unofficial, with these weight files.
    pub fn dev_bundle(root: &Path, id: &str, threshold: f32, files: &[(&str, &str, &[u8])]) -> PathBuf {
        let dir = root.join(id);
        fs::create_dir_all(dir.join("weights")).unwrap();
        let mut manifest = format!(
            "schema = \"openworld.bundle.v1\"\nid = \"{id}\"\nname = \"Dev\"\nversion = \"0\"\nofficial = false\nunofficial = true\nunofficial_notice = \"Unofficial developer bundle.\"\nbest_for = \"Tests.\"\nthreshold = {threshold}\nestimate_factor = 1.0\n\n[models]\ndetector = \"YuNet\"\ndetector_version = \"t\"\nembedder = \"SFace\"\nembedder_version = \"t\"\n"
        );
        for (role, file, bytes) in files {
            fs::write(dir.join("weights").join(file), bytes).unwrap();
            manifest.push_str(&format!(
                "\n[[files]]\nrole = \"{role}\"\nname = \"{file}\"\nversion = \"t\"\nsha256 = \"{}\"\nlicense = \"Apache-2.0\"\npath = \"weights/{file}\"\n",
                sha256_hex(bytes)
            ));
        }
        fs::write(dir.join("manifest.toml"), manifest).unwrap();
        dir
    }

    fn edit(dir: &Path, from: &str, to: &str) {
        let path = dir.join("manifest.toml");
        let text = fs::read_to_string(&path).unwrap().replace(from, to);
        fs::write(path, text).unwrap();
    }

    #[test]
    fn the_published_catalog_loads_with_fast_first() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundles");
        let all = load_bundles(&root).unwrap();
        assert_eq!(all[0].id, FAST_ID);
        assert!(all.iter().all(|b| b.official));
        assert!(all.iter().all(|b| !b.real_posters_allowed()));
    }

    #[test]
    fn a_folder_cannot_claim_to_be_official() {
        let root = tempfile::tempdir().unwrap();
        let dir = dev_bundle(root.path(), "fast", 0.5, &[]);
        edit(&dir, "official = false", "official = true");
        assert_eq!(load_bundle(&dir).unwrap_err().code, "not_official");
    }

    #[test]
    fn a_misnamed_duplicate_or_out_of_range_bundle_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let dir = dev_bundle(root.path(), "aaa", 0.5, &[]);
        edit(&dir, "id = \"aaa\"", "id = \"fast\"");
        assert_eq!(load_bundle(&dir).unwrap_err().code, "bundle_folder");
        for bad in ["-1", "1.5", "nan"] {
            let dir = dev_bundle(root.path(), "dev", 0.5, &[]);
            edit(&dir, "threshold = 0.5", &format!("threshold = {bad}"));
            assert_eq!(load_bundle(&dir).unwrap_err().code, "bundle_threshold", "{bad}");
        }
        let two = tempfile::tempdir().unwrap();
        dev_bundle(two.path(), "dev", 0.5, &[]);
        let copy = two.path().join("dev2");
        fs::create_dir_all(&copy).unwrap();
        fs::copy(two.path().join("dev/manifest.toml"), copy.join("manifest.toml")).unwrap();
        assert_eq!(load_bundles(two.path()).unwrap_err().code, "bundle_folder");
    }

    #[test]
    fn weights_stay_inside_the_folder_and_load_the_bytes_that_were_hashed() {
        let root = tempfile::tempdir().unwrap();
        let dir = dev_bundle(root.path(), "dev", 0.5, &[("detector", "det.onnx", b"detector"), ("embedder", "emb.onnx", b"embedder")]);
        let bundle = load_bundle(&dir).unwrap();
        assert!(bundle.weights_ready);
        assert_eq!(bundle.verified_bytes("detector").unwrap(), b"detector");
        fs::write(dir.join("weights/det.onnx"), b"swapped").unwrap();
        assert_eq!(bundle.verified_bytes("detector").unwrap_err().code, "weight_hash");
        edit(&dir, "path = \"weights/det.onnx\"", "path = \"/etc/hostname\"");
        assert_eq!(load_bundle(&dir).unwrap_err().code, "weight_path");
        edit(&dir, "path = \"/etc/hostname\"", "path = \"../outside.onnx\"");
        assert_eq!(load_bundle(&dir).unwrap_err().code, "weight_path");
    }

    #[test]
    fn a_research_only_file_that_is_present_is_not_a_bundle() {
        let root = tempfile::tempdir().unwrap();
        let dir = dev_bundle(root.path(), "dev", 0.5, &[("detector", "det.onnx", b"x")]);
        edit(&dir, "license = \"Apache-2.0\"", "license = \"InsightFace pretrained models are non-commercial research only.\"");
        assert_eq!(load_bundle(&dir).unwrap_err().code, "research_only");
    }

    #[test]
    fn a_curve_outside_the_bundle_folder_is_not_read() {
        let root = tempfile::tempdir().unwrap();
        let dir = dev_bundle(root.path(), "dev", 0.5, &[]);
        let curve = r#"{"schema": "openworld.curve.v1", "picker_line": "Measured.", "real_posters_allowed": true, "chance": {"fmr_at_cutoff": 1e-6, "source": "LFW"}}"#;
        fs::write(root.path().join("outside.json"), curve).unwrap();
        fs::write(dir.join("curve.json"), curve).unwrap();
        fs::write(dir.join("manifest.toml"), fs::read_to_string(dir.join("manifest.toml")).unwrap() + "")
            .unwrap();
        edit(&dir, "estimate_factor = 1.0", "estimate_factor = 1.0\ncurve = \"../outside.json\"");
        assert!(load_bundle(&dir).unwrap().curve.is_none());
        edit(&dir, "curve = \"../outside.json\"", "curve = \"curve.json\"");
        let bundle = load_bundle(&dir).unwrap();
        let curve = bundle.curve.as_ref().unwrap();
        assert_eq!(curve.fmr, Some(1e-6));
        // An unofficial bundle's curve never turns real posters on.
        assert!(curve.real_posters_allowed && !bundle.real_posters_allowed());
    }

    #[test]
    fn an_unreadable_catalog_names_the_refusal() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_bundles(&dir.path().join("none")).unwrap_err(), Line::of(copy::R_CATALOG));
    }
}
