// SPDX-License-Identifier: Apache-2.0

//! ONNX Runtime: where the library comes from, and which execution provider loads.
//!
//! The library is loaded at run time, never downloaded while building. It is
//! looked for, in order: `OPENWORLD_ORT_LIB`, next to the program (and in an
//! app bundle's Frameworks folder), the copy `scripts/fetch_ort.py` pinned for
//! development, then the system loader (an Android APK's own `lib/` folder).
//!
//! The warning a person sees follows the provider that actually registered on
//! the session, never a guess from the hardware.

use crate::hardware::{execution_from_provider, Execution};
use ort::ep::ExecutionProviderDispatch;
use ort::session::Session;
use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(target_os = "windows")]
const LIB_NAME: &str = "onnxruntime.dll";
#[cfg(any(target_os = "macos", target_os = "ios"))]
const LIB_NAME: &str = "libonnxruntime.dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios")))]
const LIB_NAME: &str = "libonnxruntime.so";

/// Paths tried for the runtime library, most specific first.
pub fn library_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(path) = std::env::var_os("OPENWORLD_ORT_LIB").filter(|p| !p.is_empty()) {
        out.push(PathBuf::from(path));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)) {
        out.push(dir.join(LIB_NAME));
        out.push(dir.join("lib").join(LIB_NAME));
        out.push(dir.join("../Frameworks").join(LIB_NAME));
        out.push(dir.join("../lib").join(LIB_NAME));
    }
    if let Some(dev) = option_env!("OPENWORLD_ORT_DEV") {
        out.push(PathBuf::from(dev));
    }
    out
}

/// Load the runtime once. A missing or too-old library is a refusal, not a panic.
pub fn ensure() -> Result<(), String> {
    static READY: OnceLock<Result<(), String>> = OnceLock::new();
    READY.get_or_init(load).clone()
}

#[cfg(feature = "ort-dynamic")]
fn load() -> Result<(), String> {
    const REFUSAL: &str = "ONNX Runtime did not load. Refusing.";
    let found = library_candidates().into_iter().find(|path| path.is_file());
    // The system loader finds a library in an APK's lib folder by its bare name.
    let path = found.unwrap_or_else(|| PathBuf::from(LIB_NAME));
    // ort panics when the library is too old for the API it asks for.
    let committed = std::panic::catch_unwind(|| ort::init_from(&path).map(|builder| builder.commit()));
    match committed {
        Ok(Ok(true)) => probe_api(),
        _ => Err(REFUSAL.into()),
    }
}

#[cfg(not(feature = "ort-dynamic"))]
fn load() -> Result<(), String> {
    // Linked statically (iOS). The library is part of this binary.
    if ort::init().commit() {
        probe_api()
    } else {
        Err("ONNX Runtime did not load. Refusing.".into())
    }
}

fn probe_api() -> Result<(), String> {
    std::panic::catch_unwind(|| {
        let _ = ort::api();
    })
    .map_err(|_| "This ONNX Runtime is too old for OpenWorld. Refusing.".to_string())
}

pub fn linked() -> bool {
    ensure().is_ok()
}

/// One provider this build can ask for, with the name ONNX Runtime reports.
struct Candidate {
    name: &'static str,
    dispatch: fn() -> ExecutionProviderDispatch,
}

/// Providers in the order a session tries them on this platform. CPU is always last
/// and is not in this list: it is what a session gets when none of these register.
fn candidates(accelerate: bool) -> Vec<Candidate> {
    #[allow(unused_mut)]
    let mut out: Vec<Candidate> = Vec::new();
    if !accelerate {
        return out;
    }
    #[cfg(feature = "coreml")]
    out.push(Candidate { name: "CoreMLExecutionProvider", dispatch: || ort::ep::CoreML::default().build().error_on_failure() });
    #[cfg(feature = "nnapi")]
    out.push(Candidate { name: "NnapiExecutionProvider", dispatch: || ort::ep::NNAPI::default().build().error_on_failure() });
    #[cfg(feature = "cuda")]
    out.push(Candidate { name: "CUDAExecutionProvider", dispatch: || ort::ep::CUDA::default().build().error_on_failure() });
    #[cfg(feature = "directml")]
    out.push(Candidate { name: "DmlExecutionProvider", dispatch: || ort::ep::DirectML::default().build().error_on_failure() });
    out
}

/// A session committed from `bytes`, the exact bytes whose SHA-256 was checked.
/// The provider is the first that registers; the second value names it.
pub fn session(bytes: &[u8], accelerate: bool) -> Result<(Session, &'static str), String> {
    ensure()?;
    for candidate in candidates(accelerate) {
        let built = Session::builder()
            .map_err(|err| err.to_string())
            .and_then(|builder| builder.with_execution_providers([(candidate.dispatch)()]).map_err(|err| err.to_string()))
            .and_then(|mut builder| builder.commit_from_memory(bytes).map_err(|err| err.to_string()));
        if let Ok(session) = built {
            return Ok((session, candidate.name));
        }
    }
    let session = Session::builder()
        .map_err(|err| err.to_string())?
        .commit_from_memory(bytes)
        .map_err(|err| err.to_string())?;
    Ok((session, "CPUExecutionProvider"))
}

/// The warning class for a provider name.
pub fn execution(provider: &str) -> Execution {
    execution_from_provider(provider)
}

/// Provider names this build can register, CPU last.
pub fn available_providers() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = candidates(true).iter().map(|c| c.name).collect();
    names.push("CPUExecutionProvider");
    names
}
