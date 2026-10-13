#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Closing the desktop window drops a report that has not been shown."""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
from pathlib import Path

os.environ.pop("OPENWORLD_EXERCISE", None)
os.environ.pop("OPENWORLD_INPUT", None)

ROOT = Path(__file__).resolve().parents[1]


def load_desktop():
    import gi

    gi.require_version("Gtk", "4.0")
    spec = importlib.util.spec_from_file_location("openworld_gtk", ROOT / "desktop" / "openworld_gtk.py")
    if spec is None or spec.loader is None:
        raise SystemExit("desktop window could not be loaded")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sleep_proc() -> subprocess.Popen[bytes]:
    return subprocess.Popen(
        ["sleep", "30"],
        start_new_session=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def main() -> None:
    if not os.environ.get("DISPLAY"):
        raise SystemExit("no display")
    desktop = load_desktop()
    app = desktop.OpenWorld()
    app.register()
    app.activate()
    report = {
        "status": "complete",
        "summary": "Possible candidate. Not an identification.",
        "candidates": [],
        "disclosure": ["Nothing is uploaded."],
    }
    app.summary.set_text("Scanning")
    app._show_report(report)
    if app.summary.get_text() != "Possible candidate. Not an identification.":
        raise SystemExit(f"an open window hid the report: {app.summary.get_text()!r}")
    app.summary.set_text("Scanning")
    running = sleep_proc()
    try:
        if not app._own_scan_process(running) or running.poll() is not None or app.scan_proc is not running:
            raise SystemExit("an open window refused the scan it had started")
        crop = app.work / "a.png"
        crop.write_bytes(b"png")
        app.out_dir = app.work
        app._stop_scan()
        if running.poll() is None or not app.closed:
            raise SystemExit("closing the window left the scan running")
        if app._show_report(report) is not False or app.summary.get_text() != "Scanning":
            raise SystemExit(f"a late report reached the closed window: {app.summary.get_text()!r}")
        open_file = app.work / "open.png"
        open_file.write_bytes(b"png")
        app.closed = False
        app.choose_file(str(open_file))
        if app.file_label.get_text() != "open.png" or app.stack.get_visible_child_name() != "device":
            raise SystemExit(
                f"an open window refused the file: {app.file_label.get_text()!r} {app.stack.get_visible_child_name()!r}"
            )
        app._stop_scan()
        if not app.closed:
            raise SystemExit("closing the window left it open")
        late_file = app.work / "late.png"
        late_file.write_bytes(b"png")
        app.choose_file(str(late_file))
        if app.file_label.get_text() != "open.png" or app.stack.get_visible_child_name() != "device":
            raise SystemExit(
                f"a file chosen as the window closed reached the screen: {app.file_label.get_text()!r} {app.stack.get_visible_child_name()!r}"
            )
        added = app._add_crop(
            {
                "kind": "face",
                "label": "Possible candidate. Not an identification.",
                "crop": "a.png",
                "frame_label": "Frame 1.",
            }
        )
        if added is not False or app.strip.get_first_child() is not None:
            raise SystemExit("a late crop reached the closed window")
        late = sleep_proc()
        try:
            if app._own_scan_process(late) or late.poll() is None:
                raise SystemExit("a scan that started as the window closed kept running")
        finally:
            if late.poll() is None:
                late.kill()
                late.wait(timeout=2)
        app.summary.set_text("Estimate")
        app._go("estimate")
        if app.stack.get_visible_child_name() != "estimate" or app.primary.get_label() != "Analyze":
            raise SystemExit(
                f"the closed window could not sit on the estimate: {app.stack.get_visible_child_name()!r} {app.primary.get_label()!r}"
            )
        app.start_scan()
        if (
            app.stack.get_visible_child_name() != "estimate"
            or app.summary.get_text() != "Estimate"
            or app.primary.get_label() != "Analyze"
            or (app.scan_thread is not None and app.scan_thread.is_alive())
            or app.scan_proc is not None
        ):
            raise SystemExit(
                "a scan started as the window closed reached the screen: "
                f"{app.stack.get_visible_child_name()!r} {app.summary.get_text()!r} {app.primary.get_label()!r}"
            )
        app.stack.set_visible_child_name("size")
        app.history = ["choose", "device", "bundle", "size"]
        app.go_back()
        if app.stack.get_visible_child_name() != "size" or app.file_label.get_text() != "open.png":
            raise SystemExit(
                "back as the window closed left the page: "
                f"{app.stack.get_visible_child_name()!r} {app.file_label.get_text()!r}"
            )
        app.on_primary()
        if app.stack.get_visible_child_name() != "size" or app.file_label.get_text() != "open.png":
            raise SystemExit(
                "continue as the window closed left the page: "
                f"{app.stack.get_visible_child_name()!r} {app.file_label.get_text()!r}"
            )
        app.stack.set_visible_child_name("results")
        app.choose_another()
        if app.stack.get_visible_child_name() != "results" or app.file_label.get_text() != "open.png":
            raise SystemExit(
                "choose another file as the window closed left the page: "
                f"{app.stack.get_visible_child_name()!r} {app.file_label.get_text()!r}"
            )
    finally:
        if running.poll() is None:
            running.kill()
            running.wait(timeout=2)
        app.do_shutdown()
    print("desktop close ok")


if __name__ == "__main__":
    main()
