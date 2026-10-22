// SPDX-License-Identifier: Apache-2.0

//! ONNX Runtime: where the library comes from, and which execution provider loads.
//!
//! The library is loaded at run time, never downloaded while building. It is
//! looked for, in order: `OPENWORLD_ORT_LIB` (when set, nothing else is
//! tried), next to the program (and in an app bundle's Frameworks folder), then
//! the copy `scripts/fetch_ort.py` pinned for development.
//!
//! Whichever file is found first, its bytes are hashed before it is loaded, and
//! it is refused unless its SHA-256 is the one `onnxruntime.pins` names for the
//! platform this core was built for. The system loader is not used on a
//! computer, because the bytes it would pick cannot be checked. Two platforms
//! differ:
//!
//! - Android loads the library by name from the app's own signed APK, where it
//!   is stored uncompressed and mapped in place, so there is no file to hash.
//!   Gradle dependency verification pins the AAR it came from at build time,
//!   and the APK signature covers it after that.
//! - iOS and the Mac app link ONNX Runtime statically (no `ort-dynamic`). Its
//!   archive is pinned by SHA-256 in `apple/scripts/build-core.sh`, and the
//!   code signature covers the app after that.
//!
//! The check is against a wrong, stale, or swapped library on disk. Someone who
//! can rewrite the file between the hash and the load already controls the
//! device, which the threat model leaves out of scope.
//!
//! The warning a person sees follows the provider that actually registered on
//! the session, never a guess from the hardware.

use crate::hardware::{execution_from_provider, Execution};
use ort::ep::ExecutionProviderDispatch;
use ort::session::Session;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/ort_pin.rs"));

#[cfg(target_os = "windows")]
const LIB_NAME: &str = "onnxruntime.dll";
#[cfg(any(target_os = "macos", target_os = "ios"))]
const LIB_NAME: &str = "libonnxruntime.dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios")))]
const LIB_NAME: &str = "libonnxruntime.so";

/// Paths tried for the runtime library, most specific first.
pub fn library_candidates() -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os("OPENWORLD_ORT_LIB").filter(|p| !p.is_empty()) {
        return vec![PathBuf::from(path)];
    }
    let mut out = Vec::new();
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

/// The `onnxruntime.pins` platform this build loads ONNX Runtime for, and the only
/// library SHA-256 it accepts. `None` when it loads none from a file.
pub fn pinned_library() -> Option<(&'static str, &'static str)> {
    ORT_PLATFORM.zip(ORT_LIBRARY_SHA256)
}

/// Refuse a library whose bytes are not the pinned build. Reads the whole file.
pub fn check_library(path: &Path, pin: Option<&str>) -> Result<(), String> {
    let Some(pin) = pin else {
        return Err("No ONNX Runtime library is pinned for this platform. Refusing.".into());
    };
    let got = sha256_file(path).map_err(|_| format!("ONNX Runtime at {} could not be read. Refusing.", path.display()))?;
    if got == pin {
        Ok(())
    } else {
        Err(format!(
            "ONNX Runtime at {} is not the pinned build: its SHA-256 is {got}, the pin is {pin}. Refusing.",
            path.display()
        ))
    }
}

/// The first candidate that exists, when its bytes are the pinned build. Nothing after it is
/// tried, and the system loader is never asked: a library it found could not be checked.
pub fn choose_library(candidates: &[PathBuf], pin: Option<&str>) -> Result<PathBuf, String> {
    let Some(found) = candidates.iter().find(|path| path.is_file()) else {
        return Err(format!(
            "ONNX Runtime ({LIB_NAME}) is not next to the program or at OPENWORLD_ORT_LIB. The system loader is not used, because its bytes cannot be checked. Refusing."
        ));
    };
    check_library(found, pin)?;
    Ok(found.clone())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut block = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        hasher.update(&block[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Load the runtime once. A missing or too-old library is a refusal, not a panic.
pub fn ensure() -> Result<(), String> {
    static READY: OnceLock<Result<(), String>> = OnceLock::new();
    READY.get_or_init(load).clone()
}

#[cfg(feature = "ort-dynamic")]
fn load() -> Result<(), String> {
    let path = library_path()?;
    // ort panics when the library is too old for the API it asks for.
    let committed = std::panic::catch_unwind(|| ort::init_from(&path).map(|builder| builder.commit()));
    match committed {
        Ok(Ok(true)) => probe_api(),
        _ => Err("ONNX Runtime did not load. Refusing.".into()),
    }
}

/// A file whose bytes were checked against the pin just before it is loaded.
#[cfg(all(feature = "ort-dynamic", not(target_os = "android")))]
fn library_path() -> Result<PathBuf, String> {
    choose_library(&library_candidates(), ORT_LIBRARY_SHA256)
}

/// The library inside this app's signed APK, by its bare name. See the module notes.
#[cfg(all(feature = "ort-dynamic", target_os = "android"))]
fn library_path() -> Result<PathBuf, String> {
    Ok(PathBuf::from(LIB_NAME))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn the_built_in_pin_is_the_library_line_in_the_pins_file() {
        let text = include_str!("../onnxruntime.pins");
        let Some((platform, pin)) = pinned_library() else {
            // A target that loads no library from a file (Android, iOS) carries no pin.
            assert!(ORT_LIBRARY_SHA256.is_none());
            return;
        };
        let lines: Vec<Vec<&str>> = text
            .lines()
            .map(|l| l.split('#').next().unwrap().split_whitespace().collect::<Vec<_>>())
            .filter(|f| f.len() == 5 && f[0] == platform && f[1] == "library")
            .collect();
        assert_eq!(lines.len(), 1, "{platform}");
        assert_eq!(lines[0][2], pin);
    }

    #[test]
    fn only_the_pinned_bytes_pass() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join(LIB_NAME);
        std::fs::write(&lib, b"pretend runtime").unwrap();
        let pin = sha(b"pretend runtime");
        assert!(check_library(&lib, Some(&pin)).is_ok());

        // One byte changed is another library.
        std::fs::write(&lib, b"pretend runtimf").unwrap();
        let refused = check_library(&lib, Some(&pin)).unwrap_err();
        assert!(refused.contains("not the pinned build") && refused.contains(&pin) && refused.ends_with("Refusing."), "{refused}");
        // No pin for the platform refuses every library.
        assert!(check_library(&lib, None).unwrap_err().contains("No ONNX Runtime library is pinned"));
        // A file that cannot be read is refused, not skipped.
        assert!(check_library(&dir.path().join("absent"), Some(&pin)).unwrap_err().contains("could not be read"));
    }

    #[test]
    fn the_first_library_found_is_the_one_checked() {
        let dir = tempfile::tempdir().unwrap();
        let (wrong, right) = (dir.path().join("a").join(LIB_NAME), dir.path().join("b").join(LIB_NAME));
        for (path, bytes) in [(&wrong, b"other".as_slice()), (&right, b"pinned".as_slice())] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let pin = sha(b"pinned");
        let missing = dir.path().join("none").join(LIB_NAME);
        assert_eq!(choose_library(&[missing.clone(), right.clone()], Some(&pin)).unwrap(), right);
        // A wrong library first is a refusal; a pinned one after it is not used instead.
        assert!(choose_library(&[wrong, right], Some(&pin)).unwrap_err().contains("not the pinned build"));
        // Nothing found: the system loader is not asked.
        let refused = choose_library(&[missing], Some(&pin)).unwrap_err();
        assert!(refused.contains("system loader is not used"), "{refused}");
    }

    #[test]
    fn the_development_copy_is_the_pinned_build() {
        let (Some(dev), Some(pin)) = (option_env!("OPENWORLD_ORT_DEV"), ORT_LIBRARY_SHA256) else {
            return;
        };
        check_library(Path::new(dev), Some(pin)).unwrap();
    }
}
