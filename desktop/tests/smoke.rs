// SPDX-License-Identifier: Apache-2.0

//! Open the window on a display and quit: `xvfb-run -a cargo test`.
//! Without a display it skips, unless `OPENWORLD_REQUIRE_DISPLAY` is set (CI).

use std::process::Command;

#[test]
fn the_window_opens_reads_startup_and_quits() {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        assert!(std::env::var_os("OPENWORLD_REQUIRE_DISPLAY").is_none(), "no display: run under xvfb-run -a");
        eprintln!("skipped: no display");
        return;
    }
    let data = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_openworld-desktop")).arg("--self-check").env("XDG_DATA_HOME", data.path()).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("self-check ok"), "{stdout}");
    assert!(stdout.contains("fast"), "the repository bundles were not found: {stdout}");
}
