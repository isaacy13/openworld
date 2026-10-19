// SPDX-License-Identifier: Apache-2.0

//! The typed request a shell sends to the library, as JSON.
//!
//! `{"v": 1, "op": "...", ...}`. Unknown fields are refused, so a misspelled
//! option is an error and never silently ignored. Every answer is JSON. A
//! refusal carries `"ok": false` and the line to show.
//!
//! A scan of frames the shell decodes itself is not a call: it is a job (see `ffi`).

use crate::copy::{self, Line};
use crate::engine::{Event, StopFlag};
use crate::scan::{estimate_for, leave_prompt, scan_path, ScanRequest};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const API_VERSION: u64 = 1;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Call {
    /// The bundle picker rows, Fast first.
    Bundles { bundles: PathBuf },
    /// The plan for this file: time, detection size, notes.
    Estimate(ScanRequest),
    /// Decode the file here and scan it. Desktop only: a phone pushes frames.
    Scan(ScanRequest),
    PostersCheck { posters: PathBuf },
    /// Write the demo poster pack. Demo builds only.
    PostersDemo { out: PathBuf },
    /// Fetch real posters. Stays off until an official curve allows it.
    PostersUpdate { bundles: PathBuf },
    Leave { url: String },
    Delete { out: PathBuf },
    /// Remove expired results under `root`.
    Sweep { root: PathBuf },
    /// Every sentence, by code.
    Copy {},
    Version {},
}

pub fn refusal(line: Line) -> Value {
    json!({ "ok": false, "status": "refused", "refusal": line.code.clone(), "headline": line })
}

/// Parse and run one call. `events` receives scan events; `stop` can end a scan.
pub fn call(text: &str, stop: Option<&StopFlag>, events: &mut dyn FnMut(Event)) -> Value {
    let parsed = match parse(text) {
        Ok(call) => call,
        Err(line) => return refusal(line),
    };
    run(parsed, stop, events)
}

pub fn parse(text: &str) -> Result<Call, Line> {
    let mut value: Value = serde_json::from_str(text).map_err(|_| Line::of(copy::R_BAD_REQUEST))?;
    let object = value.as_object_mut().ok_or_else(|| Line::of(copy::R_BAD_REQUEST))?;
    if object.remove("v").and_then(|v| v.as_u64()) != Some(API_VERSION) {
        return Err(Line::with("bad_version", "This shell and the scan library are different versions. Refusing.".into(), &[]));
    }
    serde_json::from_value(value).map_err(|err| Line::with(copy::R_BAD_REQUEST.0, format!("{} {err}", copy::R_BAD_REQUEST.1), &[]))
}

pub fn run(call: Call, stop: Option<&StopFlag>, events: &mut dyn FnMut(Event)) -> Value {
    match call {
        Call::Bundles { bundles } => match crate::bundle::load_bundles(&bundles) {
            Ok(all) if all.is_empty() => refusal(Line::of(copy::R_NO_PROGRAM)),
            Ok(all) => json!({
                "ok": true,
                "bundles": all.iter().map(crate::bundle::row_json).collect::<Vec<_>>(),
                "note": Line::of(copy::BUNDLE_NOTE),
            }),
            Err(line) => refusal(line),
        },
        Call::Estimate(req) => match estimate_for(&req) {
            Ok(estimate) => {
                let mut value = serde_json::to_value(estimate).unwrap_or(Value::Null);
                value["ok"] = true.into();
                value["classes"] = serde_json::to_value(Line::of(copy::estimate_class_line(req.missing, req.wanted))).unwrap_or_default();
                value["can_start"] = (req.missing || req.wanted).into();
                value["disclosure"] = serde_json::to_value(crate::report::disclosure(true)).unwrap_or_default();
                value
            }
            Err(report) => refusal(report.headline),
        },
        Call::Scan(req) => {
            if !(req.missing || req.wanted) {
                return refusal(Line::of(copy::CHOOSE_CLASS));
            }
            let report = scan_path(&req, stop, events);
            let mut value = serde_json::to_value(&report).unwrap_or(Value::Null);
            value["ok"] = (!report.is_refused()).into();
            value
        }
        Call::PostersCheck { posters } => match crate::posters::load_pack(&posters, std::time::SystemTime::now()) {
            Ok(pack) => json!({ "ok": true, "posters": crate::engine::pack_info(&pack) }),
            Err(line) => refusal(line),
        },
        Call::PostersDemo { out } => demo_pack(&out),
        Call::PostersUpdate { bundles } => {
            let allowed = crate::bundle::load_bundles(&bundles).map(|all| all.iter().any(|b| b.real_posters_allowed())).unwrap_or(false);
            match crate::update::update_posters(allowed, &mut crate::update::NoNetwork) {
                Ok(draft) => json!({ "ok": true, "posters": draft.posters.len(), "body_sha256": draft.body_sha256 }),
                Err(_) => refusal(Line::of(copy::R_REAL_POSTERS_OFF)),
            }
        }
        Call::Leave { url } => match leave_prompt(&url) {
            Ok(prompt) => json!({ "ok": true, "message": prompt.message, "url": prompt.url }),
            Err(line) => refusal(line),
        },
        Call::Delete { out } => match crate::scan::delete_output(&out) {
            Ok(line) => json!({ "ok": true, "deleted": true, "headline": line }),
            Err(line) => refusal(line),
        },
        Call::Sweep { root } => json!({ "ok": true, "removed": crate::outdir::sweep(&root, std::time::SystemTime::now()) }),
        Call::Copy {} => json!({ "ok": true, "copy": copy::product_copy() }),
        Call::Version {} => json!({
            "ok": true,
            "api": API_VERSION,
            "library": env!("CARGO_PKG_VERSION"),
            "demo_posters": cfg!(feature = "fixture"),
            "providers": crate::runtime::available_providers(),
        }),
    }
}

#[cfg(feature = "fixture")]
fn demo_pack(out: &std::path::Path) -> Value {
    match crate::posters::write_fixture_pack(out, std::time::SystemTime::now()) {
        Ok(pack) => json!({ "ok": true, "posters": crate::engine::pack_info(&pack), "note": Line::of(copy::FIXTURE_MARKERS) }),
        Err(line) => refusal(line),
    }
}

#[cfg(not(feature = "fixture"))]
fn demo_pack(_out: &std::path::Path) -> Value {
    refusal(Line::of(copy::R_FIXTURE_OFF))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(text: &str) -> Value {
        call(text, None, &mut |_| {})
    }

    #[test]
    fn a_request_must_name_its_version_and_known_fields() {
        assert_eq!(ask("not json")["refusal"], "bad_request");
        assert_eq!(ask(r#"{"op":"copy"}"#)["refusal"], "bad_version");
        assert_eq!(ask(r#"{"v":1,"op":"copy"}"#)["ok"], true);
        assert_eq!(ask(r#"{"v":1,"op":"nope"}"#)["refusal"], "bad_request");
        assert_eq!(ask(r#"{"v":1,"op":"copy","nope":1}"#)["refusal"], "bad_request");
        // A misspelled option is refused, not ignored.
        assert_eq!(ask(r#"{"v":1,"op":"leave","url":"https://www.fbi.gov/x","ur":"y"}"#)["refusal"], "bad_request");
        assert_eq!(ask(r#"{"v":1,"op":"leave","url":"https://www.fbi.gov/x"}"#)["url"], "https://www.fbi.gov/x");
        assert_eq!(ask(r#"{"v":1,"op":"leave","url":"https://evil.example"}"#)["refusal"], "only_fbi");
    }

    #[test]
    fn real_posters_stay_off() {
        let bundles = env!("CARGO_MANIFEST_DIR").to_string() + "/../../bundles";
        let answer = ask(&format!(r#"{{"v":1,"op":"posters_update","bundles":{bundles:?}}}"#));
        assert_eq!(answer["refusal"], "real_posters_off");
    }

    #[test]
    fn a_scan_needs_a_class() {
        let answer = ask(r#"{"v":1,"op":"scan","input":"x","bundles":"b","posters":"p","out":"o","missing":false,"wanted":false}"#);
        assert_eq!(answer["refusal"], "choose_class");
    }
}
