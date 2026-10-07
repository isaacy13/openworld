// SPDX-License-Identifier: Apache-2.0

//! On-device scan for OpenWorld.
//!
//! Shells in `apple/`, `android/`, and `desktop/` call this library (or the
//! `openworld` CLI, which is a thin wrapper). They do not detect, track, or compare.

pub mod bundle;
pub mod copy;
pub mod decode;
pub mod embed;
pub mod estimate;
pub mod fiducial;
pub mod geom;
pub mod hardware;
pub mod measure;
pub mod posters;
pub mod scan;
pub mod scene;
pub mod timeutil;
pub mod track;
pub mod update;

pub use bundle::{load_bundles, Bundle};
pub use estimate::{estimate, Coverage, DetectionSize, FormFactor};
pub use hardware::{execution_from_provider, loaded_execution, Execution};
pub use measure::measure_fast;
pub use posters::{load_pack, write_fixture_pack, PosterPack};
pub use scan::{delete_output, leave_prompt, media_warnings, scan_path, ScanReport, ScanRequest};
