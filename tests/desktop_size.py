#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""A 12-megapixel still at 640 px says which faces it leaves out, before and after the scan."""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path

os.environ.pop("OPENWORLD_EXERCISE", None)
os.environ.pop("OPENWORLD_INPUT", None)

ROOT = Path(__file__).resolve().parents[1]
NOTE = "At 640 px, a face under 404 px in this file is left out. Full resolution compares a face from 112 px."


def load_desktop():
    import gi

    gi.require_version("Gtk", "4.0")
    spec = importlib.util.spec_from_file_location("openworld_gtk", ROOT / "desktop" / "openworld_gtk.py")
    if spec is None or spec.loader is None:
        raise SystemExit("desktop window could not be loaded")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def pump(context, ticks: int = 12) -> None:
    for _ in range(ticks):
        while context.pending():
            context.iteration(False)
        time.sleep(0.02)


def main() -> None:
    if not os.environ.get("DISPLAY"):
        raise SystemExit("no display")
    desktop = load_desktop()
    app = desktop.OpenWorld()
    app.register()
    app.activate()
    from gi.repository import GLib

    context = GLib.MainContext.default()
    with tempfile.TemporaryDirectory(prefix="openworld-size-") as tmp:
        photo = Path(tmp) / "photo.jpg"
        subprocess.run(
            ["ffmpeg", "-y", "-v", "error", "-f", "lavfi", "-i", "color=c=gray:s=4032x3024", "-frames:v", "1", str(photo)],
            check=True,
        )
        try:
            pump(context, 16)
            app.load_bundles()
            app.input_path = str(photo)
            app.long_side = "640"
            app.coverage = "complete"
            app.refresh_estimate()
            lines = app.estimate_body.get_text().split("\n")
            choice = "Fast. 640 px on the long side. Every decoded frame."
            if choice not in lines or lines.index(choice) + 1 >= len(lines) or lines[lines.index(choice) + 1] != NOTE:
                raise SystemExit(f"the 640 px estimate did not name the faces it leaves out: {lines!r}")
            app.long_side = "full"
            app.refresh_estimate()
            if "left out" in app.estimate_body.get_text():
                raise SystemExit(f"a full-resolution estimate warned about size: {app.estimate_body.get_text()!r}")

            posters = Path(tmp) / "posters"
            out = Path(tmp) / "result"
            bin_path = desktop.find_bin()
            bundles = desktop.find_bundles()
            subprocess.run([bin_path, "--bundles", bundles, "posters", "write-fixture", "--out", str(posters)], check=True, capture_output=True)
            scanned = subprocess.run(
                [bin_path, "--json", "--bundles", bundles, "scan", "--input", str(photo), "--posters", str(posters), "--out", str(out)],
                check=True,
                capture_output=True,
                text=True,
            )
            app.history = ["choose", "results"]
            app._show_report(json.loads(scanned.stdout))
            app._show("results")
            pump(context, 8)
            if not app.warning_note.get_visible() or NOTE not in app.warning_note.get_text().split("\n"):
                raise SystemExit(f"the result did not name the faces it left out: {app.warning_note.get_text()!r}")
        finally:
            app.do_shutdown()
    print("desktop size ok")


if __name__ == "__main__":
    main()
