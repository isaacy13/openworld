// SPDX-License-Identifier: Apache-2.0

//! A bundle is a folder: manifest, optional ONNX weights, a locked threshold,
//! and a one-line best-for. Add or remove a model by adding or removing a folder.
//!
//! InsightFace pretrained weights are non-commercial research only. They are
//! pinned by name, version, and license, and they are not in the tree. An empty
//! SHA-256 means the digest is not pinned yet; a weight file without a pin is refused.

use crate::copy::NOT_MEASURED;
use crate::posters::sha256_hex;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const FAST_ID: &str = "fast";
pub const ACCURATE_ID: &str = "accurate";

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("bundle catalog could not be read")]
    Unreadable,
    #[error("bundle {0} is unofficial and the manifest does not say so")]
    UnofficialUnstated(String),
    #[error("bundle {0} has a weight file with no pinned SHA-256")]
    UnpinnedWeight(String),
    #[error("bundle {0} weight hash does not match")]
    BadWeightHash(String),
    #[error("bundle {0} weight license does not allow shipping it")]
    ResearchOnly(String),
}

impl BundleError {
    /// The sentence a person reads when this catalog cannot be used.
    pub fn refusal(&self) -> String {
        match self {
            BundleError::Unreadable => "The bundle catalog could not be read. Refusing.".to_string(),
            BundleError::UnofficialUnstated(id) => {
                format!("Bundle {id} is unofficial and the manifest does not say so. Refusing.")
            }
            BundleError::UnpinnedWeight(id) => {
                format!("Bundle {id} has a weight file with no pinned SHA-256. Refusing.")
            }
            BundleError::BadWeightHash(id) => {
                format!("The bundle {id} weight hash does not match. Refusing.")
            }
            BundleError::ResearchOnly(id) => {
                format!("The bundle {id} weight license does not allow shipping it. Refusing.")
            }
        }
    }
}

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

#[derive(Clone, Debug)]
pub struct Bundle {
    pub id: String,
    pub name: String,
    pub version: String,
    pub official: bool,
    pub best_for: String,
    pub threshold: f32,
    pub estimate_factor: f64,
    pub detector: String,
    pub detector_version: String,
    pub embedder: String,
    pub embedder_version: String,
    pub plate: String,
    pub plate_version: String,
    pub plate_license: String,
    pub weights: Vec<WeightPin>,
    pub weights_ready: bool,
    pub curve_exists: bool,
    pub real_posters_allowed: bool,
    pub curve_line: String,
    pub dir: PathBuf,
}

impl Bundle {
    pub fn preselected(&self) -> bool {
        self.id == FAST_ID
    }
}

#[derive(Deserialize)]
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
struct Models {
    detector: String,
    detector_version: String,
    embedder: String,
    embedder_version: String,
    plate: String,
    plate_version: String,
    plate_license: String,
}

#[derive(Deserialize)]
struct WeightFile {
    role: String,
    name: String,
    version: String,
    #[serde(default)]
    sha256: String,
    license: String,
    path: String,
}

pub fn load_bundles(dir: &Path) -> Result<Vec<Bundle>, BundleError> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|_| BundleError::Unreadable)? {
        let entry = entry.map_err(|_| BundleError::Unreadable)?;
        if entry.path().join("manifest.toml").is_file() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names.sort();
    let mut bundles = Vec::new();
    for name in names {
        bundles.push(load_bundle(&dir.join(&name))?);
    }
    // The bundle that will run is the first row. The rest stay in name order.
    bundles.sort_by_key(|bundle| !bundle.preselected());
    Ok(bundles)
}

pub fn load_bundle(dir: &Path) -> Result<Bundle, BundleError> {
    let manifest_path = dir.join("manifest.toml");
    let text = fs::read_to_string(&manifest_path).map_err(|_| BundleError::Unreadable)?;
    let manifest: Manifest = toml::from_str(&text).map_err(|_| BundleError::Unreadable)?;
    if manifest.schema != "openworld.bundle.v1" {
        return Err(BundleError::Unreadable);
    }
    if !manifest.official && (!manifest.unofficial || !manifest.unofficial_notice.to_ascii_lowercase().contains("unofficial"))
    {
        return Err(BundleError::UnofficialUnstated(manifest.id));
    }
    let mut weights = Vec::new();
    let mut ready = !manifest.files.is_empty();
    for file in &manifest.files {
        let path = dir.join(&file.path);
        let present = path.is_file();
        if present {
            if license_forbids_shipping(&file.license) {
                return Err(BundleError::ResearchOnly(manifest.id.clone()));
            }
            if file.sha256.trim().is_empty() {
                return Err(BundleError::UnpinnedWeight(manifest.id.clone()));
            }
            let bytes = fs::read(&path).map_err(|_| BundleError::Unreadable)?;
            if sha256_hex(&bytes) != file.sha256.trim() {
                return Err(BundleError::BadWeightHash(manifest.id.clone()));
            }
        } else {
            ready = false;
        }
        weights.push(WeightPin {
            role: file.role.clone(),
            name: file.name.clone(),
            version: file.version.clone(),
            sha256: file.sha256.clone(),
            license: file.license.clone(),
            path: file.path.clone(),
            present,
        });
    }
    if manifest.files.is_empty() {
        ready = false;
    }
    let (curve_exists, real_posters_allowed, curve_line) = read_curve(dir, &manifest.curve);
    Ok(Bundle {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        official: manifest.official,
        best_for: manifest.best_for,
        threshold: manifest.threshold,
        estimate_factor: manifest.estimate_factor,
        detector: manifest.models.detector,
        detector_version: manifest.models.detector_version,
        embedder: manifest.models.embedder,
        embedder_version: manifest.models.embedder_version,
        plate: manifest.models.plate,
        plate_version: manifest.models.plate_version,
        plate_license: manifest.models.plate_license,
        weights,
        weights_ready: ready,
        curve_exists,
        real_posters_allowed,
        curve_line,
        dir: dir.to_path_buf(),
    })
}

fn read_curve(bundle_dir: &Path, curve: &str) -> (bool, bool, String) {
    if curve.is_empty() {
        return (false, false, NOT_MEASURED.into());
    }
    let path = bundle_dir.join(curve);
    let Ok(text) = fs::read_to_string(&path) else {
        return (false, false, NOT_MEASURED.into());
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (false, false, NOT_MEASURED.into());
    };
    if value.get("schema").and_then(|v| v.as_str()) != Some("openworld.curve.v1") {
        return (false, false, NOT_MEASURED.into());
    }
    let allowed = value.get("real_posters_allowed").and_then(|v| v.as_bool()).unwrap_or(false);
    let line = value
        .get("picker_line")
        .and_then(|v| v.as_str())
        .unwrap_or("Curve file present.")
        .to_string();
    (true, allowed, line)
}

pub fn row_json(bundle: &Bundle) -> serde_json::Value {
    serde_json::json!({
        "id": bundle.id,
        "name": bundle.name,
        "version": bundle.version,
        "best_for": bundle.best_for,
        "preselected": bundle.preselected(),
        "threshold": bundle.threshold,
        "curve_exists": bundle.curve_exists,
        "curve_line": bundle.curve_line,
        "real_posters_allowed": bundle.real_posters_allowed,
        "weights_ready": bundle.weights_ready,
        "official": bundle.official,
        "detector": bundle.detector,
        "embedder": bundle.embedder,
        "plate": bundle.plate,
    })
}

/// InsightFace zoo files say non-commercial research only. Those bytes are not an official bundle.
pub fn license_forbids_shipping(license: &str) -> bool {
    let text = license.to_ascii_lowercase();
    text.contains("non-commercial")
        || text.contains("noncommercial")
        || text.contains("research only")
        || text.contains("research-only")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::posters::sha256_hex;

    #[test]
    fn a_research_only_file_that_is_present_is_not_a_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("fast");
        std::fs::create_dir_all(bundle.join("weights")).unwrap();
        let bytes = b"not-a-weight";
        std::fs::write(bundle.join("weights/det.onnx"), bytes).unwrap();
        let sha = sha256_hex(bytes);
        let manifest = format!(
            r#"
schema = "openworld.bundle.v1"
id = "fast"
name = "Fast"
version = "0.0.0"
official = true
best_for = "test"
threshold = 0.5
estimate_factor = 1.0

[models]
detector = "SCRFD-0.5GF"
detector_version = "x"
embedder = "ArcFace-MBF"
embedder_version = "x"
plate = "RTMDet-nano"
plate_version = "x"
plate_license = "Apache-2.0"

[[files]]
role = "detector"
name = "SCRFD-0.5GF"
version = "x"
sha256 = "{sha}"
license = "InsightFace pretrained models are non-commercial research only."
path = "weights/det.onnx"
"#
        );
        std::fs::write(bundle.join("manifest.toml"), manifest).unwrap();
        let err = load_bundle(&bundle).unwrap_err();
        assert!(matches!(err, BundleError::ResearchOnly(_)));
        assert_eq!(
            err.refusal(),
            "The bundle fast weight license does not allow shipping it. Refusing."
        );
    }

    #[test]
    fn an_unreadable_catalog_names_the_refusal() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-catalog");
        let err = load_bundles(&missing).unwrap_err();
        assert!(matches!(err, BundleError::Unreadable));
        assert_eq!(err.refusal(), "The bundle catalog could not be read. Refusing.");
    }

    #[test]
    fn the_published_fast_manifest_loads_without_weights() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundles");
        let fast = load_bundle(&root.join("fast")).unwrap();
        assert!(!fast.weights_ready);
        assert!(fast
            .weights
            .iter()
            .any(|weight| weight.role == "detector" && license_forbids_shipping(&weight.license)));
        assert!(fast.weights.iter().any(|weight| weight.role == "plate" && !license_forbids_shipping(&weight.license)));
    }
}
