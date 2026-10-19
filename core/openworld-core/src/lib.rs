// SPDX-License-Identifier: Apache-2.0

//! On-device scan for OpenWorld.
//!
//! Shells in `apple/`, `android/`, and `desktop/` call this library through
//! `ffi` (or, on a computer, link it directly). They do not detect, track, or
//! compare. The `openworld` CLI is a thin wrapper over the same calls.

pub mod arcface;
pub mod bench;
pub mod bundle;
pub mod copy;
pub mod decode;
pub mod engine;
pub mod estimate;
pub mod ffi;
#[cfg(feature = "fixture")]
pub mod fixture;
pub mod geom;
pub mod hardware;
pub mod invoke;
#[cfg(feature = "fixture")]
pub mod measure;
pub mod onnx_exec;
pub mod outdir;
pub mod perception;
pub mod posters;
pub mod quality;
pub mod report;
pub mod runtime;
pub mod scan;
pub mod scrfd;
pub mod timeutil;
pub mod track;
pub mod update;
pub mod yunet;

pub use bundle::{load_bundles, Bundle};
pub use copy::Line;
pub use engine::{DecodeEnd, Engine, Event, StopFlag, StopReason};
pub use estimate::{Coverage, Device, Estimate};
pub use hardware::{execution_from_provider, Execution};
pub use posters::{load_pack, PosterPack};
pub use report::{ScanReport, Status, TrackStatus};
pub use scan::{delete_output, estimate_for, leave_prompt, media_warnings, scan_path, MediaFacts, ScanRequest};
