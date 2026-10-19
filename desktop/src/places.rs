// SPDX-License-Identifier: Apache-2.0

//! Where the app keeps things: the bundle catalog, the poster pack, results,
//! and temporary copies of files that had no local path.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Temporary copies live in folders named `openworld-import-<pid>` in the system temp dir.
pub const IMPORT_PREFIX: &str = "openworld-import-";
/// A copy whose owner cannot be checked is kept this long.
const IMPORT_MAX_AGE: Duration = Duration::from_secs(2 * 86400);

#[derive(Clone, Debug)]
pub struct Places {
    pub bundles: PathBuf,
    pub posters: PathBuf,
    pub results: PathBuf,
    pub imports: PathBuf,
    /// The terms version the person accepted on the first-run screen. Never sent.
    pub accepted: PathBuf,
}

impl Places {
    /// `data` is the per-user data folder (`$XDG_DATA_HOME`, `%LOCALAPPDATA%`).
    pub fn new(data: &Path, bundles: PathBuf) -> Self {
        let root = data.join("openworld");
        Self {
            bundles,
            posters: root.join("posters"),
            results: root.join("results"),
            imports: std::env::temp_dir().join(format!("{IMPORT_PREFIX}{}", std::process::id())),
            accepted: root.join("accepted-terms"),
        }
    }

    /// A new result folder named for `now`, never one that already exists.
    pub fn new_result(&self, now: SystemTime) -> PathBuf {
        let stamp = openworld_core::timeutil::format_rfc3339(now).replace(':', "-");
        let mut out = self.results.join(&stamp);
        let mut n = 2;
        while out.exists() {
            out = self.results.join(format!("{stamp}-{n}"));
            n += 1;
        }
        out
    }
}

/// The bundle catalog: `OPENWORLD_BUNDLES`, then `bundles/` beside the program,
/// then the repository's `bundles/` above it (a development build).
pub fn find_bundles(env: Option<PathBuf>, exe: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = env.filter(|p| !p.as_os_str().is_empty()) {
        return dir;
    }
    let exe_dir = exe.as_deref().and_then(Path::parent).map(Path::to_path_buf);
    if let Some(dir) = &exe_dir {
        if is_catalog(&dir.join("bundles")) {
            return dir.join("bundles");
        }
        let mut cursor = dir.parent();
        while let Some(parent) = cursor {
            if is_catalog(&parent.join("bundles")) {
                return parent.join("bundles");
            }
            cursor = parent.parent();
        }
    }
    // Not found: the core refuses this path with "The scan program is not on this device."
    exe_dir.unwrap_or_default().join("bundles")
}

/// A folder holding at least one bundle manifest.
fn is_catalog(dir: &Path) -> bool {
    fs::read_dir(dir).map(|entries| entries.flatten().any(|e| e.path().join("manifest.toml").is_file())).unwrap_or(false)
}

/// Remove temporary copies left by an earlier run. Returns how many folders went.
pub fn sweep_imports(temp: &Path, now: SystemTime) -> usize {
    let Ok(entries) = fs::read_dir(temp) else {
        return 0;
    };
    let mine = std::process::id();
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(pid) = name.strip_prefix(IMPORT_PREFIX).and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        if pid == mine || !entry.path().is_dir() || owner_alive(pid, &entry.path(), now) {
            continue;
        }
        if fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// On Linux the owner is checked directly. Elsewhere a folder is kept for two days.
fn owner_alive(pid: u32, dir: &Path, now: SystemTime) -> bool {
    if cfg!(target_os = "linux") {
        return Path::new(&format!("/proc/{pid}")).exists();
    }
    let modified = fs::metadata(dir).and_then(|m| m.modified()).unwrap_or(now);
    now.duration_since(modified).map(|age| age < IMPORT_MAX_AGE).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_come_from_env_then_beside_the_program_then_above_it() {
        let root = tempfile::tempdir().unwrap();
        let catalog = root.path().join("repo/bundles/fast");
        fs::create_dir_all(&catalog).unwrap();
        fs::write(catalog.join("manifest.toml"), "x").unwrap();
        let exe = root.path().join("repo/desktop/target/debug/openworld-desktop");
        assert_eq!(find_bundles(Some("/env".into()), Some(exe.clone())), PathBuf::from("/env"));
        assert_eq!(find_bundles(None, Some(exe.clone())), root.path().join("repo/bundles"));
        let beside = root.path().join("repo/desktop/target/debug/bundles/fast");
        fs::create_dir_all(&beside).unwrap();
        fs::write(beside.join("manifest.toml"), "x").unwrap();
        assert_eq!(find_bundles(None, Some(exe)), root.path().join("repo/desktop/target/debug/bundles"));
    }

    #[test]
    fn only_dead_owners_lose_their_copies() {
        let temp = tempfile::tempdir().unwrap();
        let dead = temp.path().join(format!("{IMPORT_PREFIX}4000000000"));
        let live = temp.path().join(format!("{IMPORT_PREFIX}{}", std::process::id()));
        let other = temp.path().join("openworld-motion-x");
        for dir in [&dead, &live, &other] {
            fs::create_dir_all(dir).unwrap();
        }
        let later = SystemTime::now() + Duration::from_secs(3 * 86400);
        assert_eq!(sweep_imports(temp.path(), later), 1);
        assert!(!dead.exists() && live.exists() && other.exists());
    }

    #[test]
    fn result_folders_never_collide() {
        let root = tempfile::tempdir().unwrap();
        let places = Places::new(root.path(), root.path().join("bundles"));
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_760_000_000);
        let first = places.new_result(now);
        // The folder name only: a Windows path starts with a drive letter and a colon.
        assert!(!first.file_name().unwrap().to_string_lossy().contains(':'));
        fs::create_dir_all(&first).unwrap();
        assert_ne!(places.new_result(now), first);
    }
}
