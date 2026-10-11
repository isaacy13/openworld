#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""A crop that arrives during a scan comes onto the screen, and Scanning stays there."""

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

    raw = b"\x00" + b"\x00\x00\x00" * 8
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 8, 8, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw * 8))
        + chunk(b"IEND", b"")
    )


def bounds(window, widget):
    found = widget.compute_bounds(window)
    if not found:
        return None
    rect = found.out_bounds
    return rect.get_x(), rect.get_y(), rect.get_width(), rect.get_height()


def find_label(root, text: str):
    from gi.repository import Gtk

    if isinstance(root, Gtk.Label) and root.get_text() == text:
        return root
    child = root.get_first_child()
    while child is not None:
        found = find_label(child, text)
        if found is not None:
            return found
        child = child.get_next_sibling()
    return None


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
        pump(context, 30)
        folder = app.work / "crops"
        folder.mkdir(parents=True)
        (folder / "a.png").write_bytes(png())
        app.out_dir = app.work
        app.summary.set_text("Scanning")
        app.history = ["choose", "estimate", "results"]
        app._show("results")
        app.primary.set_label("Scanning")
        app.primary.set_sensitive(False)
        for index in range(30):
            app._add_crop(
                {
                    "kind": "face",
                    "crop": "crops/a.png",
                    "label": "Not compared.",
                    "frame_label": f"Frame {index + 1}.",
                }
            )
        pump(context, 30)
        width, height = app.window.get_width(), app.window.get_height()
        scanning = bounds(app.window, app.summary)
        arrived = find_label(app.strip, "Frame 30.")
        crop = None if arrived is None else bounds(app.window, arrived)
        on_screen = (
            crop is not None
            and crop[2] > 0
            and crop[3] > 0
            and crop[0] >= 0
            and crop[1] >= 0
            and crop[0] + crop[2] <= width
            and crop[1] + crop[3] <= height
        )
        scanning_on = (
            scanning is not None
            and scanning[3] > 0
            and scanning[1] >= 0
            and scanning[1] + scanning[3] <= height
            and app.summary.get_text() == "Scanning"
        )
        if not on_screen or not scanning_on:
            raise SystemExit(
                f"the crop that arrived left the screen: {crop}, Scanning was {scanning}, window {width}x{height}"
            )
        caption = app.strip.get_last_child().get_last_child()
        caption_bounds = bounds(app.window, caption)
        bar = bounds(app.window, app.strip_scroll.get_hscrollbar())
        covered = (
            caption_bounds is None
            or bar is None
            or caption.get_text() != "Not compared."
            or caption_bounds[1] + caption_bounds[3] > bar[1] + 1
            or caption_bounds[0] < 0
            or caption_bounds[0] + caption_bounds[2] > width
        )
        if covered:
            raise SystemExit(
                f"the crop label is under the scrollbar: {caption_bounds}, bar {bar}, window {width}x{height}"
            )
        app.summary.set_text("No candidate is not a clearance.")
        app._fill_strip(
            {
                "inventory": [
                    {
                        "crop": "crops/a.png",
                        "label": "Not compared.",
                        "frame_label": f"Frame {index}.",
                    }
                    for index in range(1, 31)
                ]
            }
        )
        app.strip_scroll.get_hadjustment().set_value(0)
        parent = app.strip.get_parent()
        if parent is not None:
            parent.queue_allocate()
        pump(context, 20)
        first_widget = find_label(app.strip, "Frame 1.")
        first = None if first_widget is None else bounds(app.window, first_widget)
        first_on = (
            first is not None
            and first[2] > 0
            and first[3] > 0
            and first[0] >= 0
            and first[0] + first[2] <= width
            and first[1] >= 0
            and first[1] + first[3] <= height
        )
        if not first_on:
            raise SystemExit(f"the finished strip did not start at the first crop: {first}")
    finally:
        app.do_shutdown()
    print("desktop crops ok")


if __name__ == "__main__":
    main()
