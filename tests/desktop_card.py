#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""The frame photo stays beside the results scrollbar."""

from __future__ import annotations

import importlib.util
import os
import struct
import time
import zlib
from pathlib import Path

os.environ.pop("OPENWORLD_EXERCISE", None)
os.environ.pop("OPENWORLD_INPUT", None)

ROOT = Path(__file__).resolve().parents[1]


def load_desktop():
    import gi

    gi.require_version("Gtk", "4.0")
    from gi.repository import Gtk

    # A compact font lets the window be narrower than the two evidence photos.
    Gtk.Settings.get_default().set_property("gtk-font-name", "Sans 9")
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


def png() -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + b"\x20\x60\xc0" * 40 for _ in range(30))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 40, 30, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def bounds(window, widget):
    found = widget.compute_bounds(window)
    if not found:
        return None
    rect = found.out_bounds
    return rect.get_x(), rect.get_y(), rect.get_width(), rect.get_height()


def pictures(root) -> list:
    from gi.repository import Gtk

    found = []
    if isinstance(root, Gtk.Picture):
        found.append(root)
    child = root.get_first_child()
    while child is not None:
        found.extend(pictures(child))
        child = child.get_next_sibling()
    return found


def overlap(a, b) -> float:
    if a is None or b is None:
        return 0
    width = min(a[0] + a[2], b[0] + b[2]) - max(a[0], b[0])
    height = min(a[1] + a[3], b[1] + b[3]) - max(a[1], b[1])
    if width <= 0 or height <= 0:
        return 0
    return width * height


def main() -> None:
    if not os.environ.get("DISPLAY"):
        raise SystemExit("no display")
    desktop = load_desktop()
    app = desktop.OpenWorld()
    app.register()
    app.activate()
    from gi.repository import GLib

    context = GLib.MainContext.default()
    try:
        pump(context, 16)
        folder = app.work / "crops"
        folder.mkdir(parents=True)
        (folder / "a.png").write_bytes(png())
        app.out_dir = app.work
        app.summary.set_text("Possible candidate. Not an identification.")
        app.history = ["choose", "results"]
        app._show("results")
        app.detail.append(
            app._candidate_card(
                {
                    "wording": "Possible candidate. Not an identification.",
                    "crop": "crops/a.png",
                    "frame": "crops/a.png",
                    "frame_label": "Frame 1.",
                    "uncertainty": "Score 0.90. Fixture markers keep a candidate at 0.55 and above.",
                    "poster_title": "Fixture subject A",
                    "poster_class_label": "Missing",
                    "fbi_url": "https://www.fbi.gov/wanted",
                }
            )
        )
        app.result_note.set_text("Nothing is uploaded.\n" * 14)
        app.result_note.set_visible(True)
        app.window.set_default_size(240, 460)
        pump(context, 18)
        shots = pictures(app.detail)
        frame = None if len(shots) < 2 else shots[1]
        frame_box = None if frame is None else bounds(app.window, frame)
        bar = app.results_scroll.get_vscrollbar()
        bar_box = bounds(app.window, bar)
        covered = overlap(frame_box, bar_box)
        visible = (
            frame_box is not None
            and bar_box is not None
            and bar.get_visible()
            and frame_box[2] >= 140
            and frame_box[3] >= 100
            and covered <= 1
            and frame_box[0] >= 0
            and frame_box[0] + frame_box[2] <= app.window.get_width() + 1
        )
        if not visible:
            raise SystemExit(
                "the frame photo sat under the results scrollbar: "
                f"frame {frame_box}, bar {bar_box}, overlap {covered}, "
                f"window {app.window.get_width()}x{app.window.get_height()}"
            )
    finally:
        app.do_shutdown()
    print("desktop card ok")


if __name__ == "__main__":
    main()
