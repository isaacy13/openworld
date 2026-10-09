#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Run the unit, snapshot, and phone checks that this machine can run.

The Rust suite owns the decision snapshot and the locked-cutoff counts.
Apple and Android run when those toolchains are installed. The desktop
window runs when a display is available.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(cmd: list[str], cwd: Path | None = None, env: dict[str, str] | None = None) -> None:
    print("+", " ".join(cmd), flush=True)
    merged = os.environ.copy()
    if env:
        merged.update(env)
    subprocess.run(cmd, cwd=cwd or ROOT, env=merged, check=True)


def binary() -> Path:
    debug = ROOT / "core/target/debug/openworld"
    release = ROOT / "core/target/release/openworld"
    if debug.is_file():
        return debug
    if release.is_file():
        return release
    raise SystemExit("openworld is not built")


def gtk_ready() -> bool:
    try:
        import gi

        gi.require_version("Gtk", "4.0")
        from gi.repository import Gtk  # noqa: F401
    except Exception:
        return False
    return True


def desktop() -> None:
    if not os.environ.get("DISPLAY"):
        print("no display; skipping the desktop window")
        return
    if not gtk_ready():
        print("GTK 4 is not installed; skipping the desktop window")
        return
    bin_path = binary()
    with tempfile.TemporaryDirectory(prefix="openworld-auto-") as tmp:
        scene = Path(tmp) / "scene.png"
        status = Path(tmp) / "status.json"
        subprocess.run(
            [str(bin_path), "--json", "--bundles", str(ROOT / "bundles"), "fixture-still", "--out", str(scene), "--scene"],
            check=True,
            cwd=ROOT,
        )
        env = os.environ.copy()
        env.update(
            {
                "OPENWORLD_EXERCISE": "1",
                "OPENWORLD_INPUT": str(scene),
                "OPENWORLD_BIN": str(bin_path),
                "OPENWORLD_BUNDLES": str(ROOT / "bundles"),
                "OPENWORLD_STATUS": str(status),
            }
        )
        print("+ desktop window", flush=True)
        subprocess.run([sys.executable, str(ROOT / "desktop/openworld_gtk.py")], cwd=ROOT, env=env, check=True)
        text = status.read_text()
        if '"ok": true' not in text and '"ok":true' not in text:
            raise SystemExit(f"desktop exercise failed: {text}")


def main() -> None:
    run(["cargo", "test", "--manifest-path", "core/Cargo.toml"])
    run([sys.executable, str(ROOT / "tests/words.py")])
    run(
        [
            sys.executable,
            str(ROOT / "eval/heldout.py"),
            "--bin",
            str(binary()),
            "--bundles",
            str(ROOT / "bundles"),
        ]
    )
    bin_path = binary()
    if shutil.which("swift"):
        apple_env = {"OPENWORLD_BIN": str(bin_path), "OPENWORLD_BUNDLES": str(ROOT / "bundles")}
        if sys.platform.startswith("linux"):
            run(
                ["sh", str(ROOT / "apple/scripts/test-linux.sh"), "--filter", "PhoneContractTests|OpenWorldUITests"],
                cwd=ROOT / "apple",
                env=apple_env,
            )
        else:
            run(["swift", "test", "--filter", "PhoneContractTests"], cwd=ROOT / "apple", env=apple_env)
    else:
        print("swift is not installed; skipping the Apple contract")
    if os.environ.get("ANDROID_HOME") and (ROOT / "android/gradlew").is_file():
        run(["./gradlew", "testDebugUnitTest", "--stacktrace"], cwd=ROOT / "android")
    else:
        print("Android SDK is not configured; skipping the phone screens")
    desktop()
    print("automation ok")


if __name__ == "__main__":
    main()
