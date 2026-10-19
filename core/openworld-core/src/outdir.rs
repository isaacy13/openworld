// SPDX-License-Identifier: Apache-2.0

//! Result folders: the core only replaces or deletes a folder it made.
//!
//! A result folder holds a marker file the moment it is created. A folder
//! without that marker is never emptied: a scan refuses it unless it is new
//! or empty, and `delete` refuses it. Results expire: `sweep` removes marked
//! folders whose `expires_at` has passed.

use crate::copy::{self, Copy, Line};
use crate::timeutil::parse_rfc3339;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const MARKER: &str = ".openworld-result";
pub const RESULT_FILE: &str = "result.json";
/// A result lives at most this long, or until its poster pack expires.
pub const RESULT_DAYS: u64 = 7;

pub fn is_result(dir: &Path) -> bool {
    dir.join(MARKER).is_file()
}

/// Make `dir` an empty result folder, refusing anything that is not ours.
/// `protect` are paths the folder must not hold or sit inside: the input file,
/// the poster pack, and the bundle catalog.
pub fn prepare(dir: &Path, protect: &[&Path]) -> Result<(), Line> {
    if creation_blocked(dir) {
        return Err(Line::of(copy::R_OUT_CREATE));
    }
    if overlaps(dir, protect) {
        return Err(Line::of(copy::R_OUT_OVERLAP));
    }
    if dir.exists() {
        let empty = fs::read_dir(dir).map(|mut entries| entries.next().is_none()).unwrap_or(false);
        if is_result(dir) {
            fs::remove_dir_all(dir).map_err(|_| Line::of(copy::R_OUT_REPLACE))?;
        } else if !empty {
            return Err(Line::of(copy::R_OUT_FOREIGN));
        }
    }
    fs::create_dir_all(dir).map_err(|_| Line::of(copy::R_OUT_CREATE))?;
    fs::write(dir.join(MARKER), b"openworld result\n").map_err(|_| Line::of(copy::R_OUT_CREATE))?;
    Ok(())
}

/// A path that cannot become a folder: empty, a file, or under a file.
/// Checked without creating or deleting anything.
pub fn creation_blocked(dir: &Path) -> bool {
    if dir.as_os_str().is_empty() || is_file(dir) {
        return true;
    }
    let mut cursor = dir.parent();
    while let Some(parent) = cursor {
        if parent.as_os_str().is_empty() || parent.exists() {
            return is_file(parent);
        }
        cursor = parent.parent();
    }
    false
}

fn is_file(path: &Path) -> bool {
    path.exists() && !path.is_dir()
}

/// The absolute form of a path whose tail may not exist yet.
fn absolute(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    while !existing.exists() {
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => break,
        }
        if existing.as_os_str().is_empty() {
            existing = PathBuf::from(".");
        }
    }
    let mut out = existing.canonicalize().unwrap_or(existing);
    for name in tail.into_iter().rev() {
        out.push(name);
    }
    out
}

fn overlaps(dir: &Path, protect: &[&Path]) -> bool {
    let out = absolute(dir);
    protect.iter().filter(|p| !p.as_os_str().is_empty()).any(|p| {
        let other = absolute(p);
        other.starts_with(&out) || (out.starts_with(&other) && other.is_dir())
    })
}

pub fn delete(dir: &Path) -> Result<Line, Line> {
    if !is_result(dir) {
        return Err(Line::of(copy::R_DELETE_FOREIGN));
    }
    fs::remove_dir_all(dir).map(|()| Line::of(copy::DELETED)).map_err(|_| Line::of(copy::R_DELETE_FAILED))
}

/// Remove every expired result folder directly under `root`. Returns how many.
/// A folder without a readable result may be a scan still running, so it is
/// kept until `RESULT_DAYS` after its marker was written: a crashed scan's
/// leftovers do not stay forever.
pub fn sweep(root: &Path, now: SystemTime) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let dir = entry.path();
        if !is_result(&dir) {
            continue;
        }
        let expired = fs::read(dir.join(RESULT_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.get("expires_at").and_then(|v| v.as_str()).and_then(parse_rfc3339))
            .or_else(|| marker_expiry(&dir))
            .is_some_and(|expires| expires <= now);
        if expired && fs::remove_dir_all(&dir).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn marker_expiry(dir: &Path) -> Option<SystemTime> {
    let written = fs::metadata(dir.join(MARKER)).and_then(|meta| meta.modified()).ok()?;
    written.checked_add(Duration::from_secs(RESULT_DAYS * 86_400))
}

pub fn refusal_copy(line: &Line) -> Option<Copy> {
    [copy::R_OUT_CREATE, copy::R_OUT_FOREIGN, copy::R_OUT_OVERLAP, copy::R_OUT_REPLACE].into_iter().find(|c| line.is(*c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_that_is_not_a_result_is_never_emptied() {
        let root = tempfile::tempdir().unwrap();
        let precious = root.path().join("precious");
        fs::create_dir_all(&precious).unwrap();
        fs::write(precious.join("thesis.docx"), b"years of work").unwrap();
        assert_eq!(prepare(&precious, &[]).unwrap_err(), Line::of(copy::R_OUT_FOREIGN));
        assert!(precious.join("thesis.docx").is_file());
        assert_eq!(delete(&precious).unwrap_err(), Line::of(copy::R_DELETE_FOREIGN));
        assert!(precious.join("thesis.docx").is_file());
    }

    #[test]
    fn a_new_or_empty_folder_becomes_a_result_and_a_result_is_replaced() {
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("a/b/result");
        prepare(&out, &[]).unwrap();
        assert!(is_result(&out));
        fs::write(out.join("crop.jpg"), b"x").unwrap();
        prepare(&out, &[]).unwrap();
        assert!(!out.join("crop.jpg").exists());
        let empty = root.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        prepare(&empty, &[]).unwrap();
        assert_eq!(delete(&empty).unwrap(), Line::of(copy::DELETED));
        assert!(!empty.exists());
    }

    #[test]
    fn a_result_cannot_hold_the_input_or_sit_inside_the_posters() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("videos/clip.mp4");
        fs::create_dir_all(input.parent().unwrap()).unwrap();
        fs::write(&input, b"video").unwrap();
        let posters = root.path().join("posters");
        fs::create_dir_all(&posters).unwrap();
        for out in [root.path().join("videos"), root.path().to_path_buf(), posters.join("result")] {
            assert_eq!(prepare(&out, &[&input, &posters]).unwrap_err(), Line::of(copy::R_OUT_OVERLAP), "{out:?}");
        }
        assert!(input.is_file());
        // Beside the input is fine.
        prepare(&root.path().join("videos-result"), &[&input, &posters]).unwrap();
    }

    #[test]
    fn a_path_under_a_file_cannot_be_created() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file");
        fs::write(&file, b"x").unwrap();
        assert!(creation_blocked(&file));
        assert!(creation_blocked(&file.join("inside")));
        assert!(creation_blocked(Path::new("")));
        assert!(!creation_blocked(&root.path().join("new/deeper")));
        assert_eq!(prepare(&file, &[]).unwrap_err(), Line::of(copy::R_OUT_CREATE));
        assert!(file.is_file());
    }

    #[test]
    fn sweep_removes_only_expired_results() {
        let root = tempfile::tempdir().unwrap();
        let now = parse_rfc3339("2026-10-19T00:00:00Z").unwrap();
        for (name, expires) in [("old", "2026-10-18T00:00:00Z"), ("new", "2026-10-25T00:00:00Z")] {
            let dir = root.path().join(name);
            prepare(&dir, &[]).unwrap();
            fs::write(dir.join(RESULT_FILE), format!("{{\"expires_at\":\"{expires}\"}}")).unwrap();
        }
        let foreign = root.path().join("foreign");
        fs::create_dir_all(&foreign).unwrap();
        fs::write(foreign.join(RESULT_FILE), "{\"expires_at\":\"2000-01-01T00:00:00Z\"}").unwrap();
        assert_eq!(sweep(root.path(), now), 1);
        assert!(!root.path().join("old").exists());
        assert!(root.path().join("new").exists());
        assert!(foreign.exists());
        assert_eq!(sweep(root.path(), now + Duration::from_secs(30 * 86400)), 1);
    }

    #[test]
    fn sweep_keeps_a_running_scan_but_not_a_crashed_one_forever() {
        let root = tempfile::tempdir().unwrap();
        prepare(&root.path().join("crashed"), &[]).unwrap();
        let now = SystemTime::now();
        assert_eq!(sweep(root.path(), now), 0);
        assert_eq!(sweep(root.path(), now + Duration::from_secs((RESULT_DAYS + 1) * 86400)), 1);
    }
}
