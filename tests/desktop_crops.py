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
        app._clear_strip()
        long = "Possible candidate. Not an identification."
        for index in range(8):
            app._add_crop(
                {
                    "kind": "face",
                    "crop": "crops/a.png",
                    "label": long,
                    "frame_label": f"Frame {index + 1}.",
                }
            )
        pump(context, 30)
        long_caption = app.strip.get_last_child().get_last_child()
        long_bounds = bounds(app.window, long_caption)
        long_bar = bounds(app.window, app.strip_scroll.get_hscrollbar())
        long_covered = (
            long_bounds is None
            or long_bar is None
            or long_caption.get_text() != long
            or long_bounds[3] < 20
            or long_bounds[1] + long_bounds[3] > long_bar[1] + 1
            or long_bounds[0] < 0
            or long_bounds[0] + long_bounds[2] > width
            or long_bounds[1] < 0
            or long_bounds[1] + long_bounds[3] > height
        )
        if long_covered:
            raise SystemExit(
                f"the long crop label is under the scrollbar: {long_bounds}, bar {long_bar}, window {width}x{height}"
            )
        from gi.repository import Gtk

        for _ in range(14):
            app.detail.append(
                Gtk.Label(label="Possible candidate. Not an identification. " * 6, wrap=True, xalign=0)
            )
        pump(context, 25)
        vadj = app.results_scroll.get_vadjustment()
        if vadj.get_upper() <= vadj.get_page_size() + 1:
            raise SystemExit(
                f"the result did not grow a vertical scrollbar: {vadj.get_upper()} {vadj.get_page_size()}"
            )
        vadj.set_value(max(0.0, vadj.get_upper() - vadj.get_page_size()))
        pump(context, 20)
        tall_caption = app.strip.get_last_child().get_last_child()
        tall_bounds = bounds(app.window, tall_caption)
        tall_bar = bounds(app.window, app.strip_scroll.get_hscrollbar())
        tall_covered = (
            tall_bounds is None
            or tall_bar is None
            or tall_caption.get_text() != long
            or tall_bounds[3] < 20
            or tall_bounds[1] < 0
            or tall_bounds[1] + tall_bounds[3] > height
            or tall_bounds[1] + tall_bounds[3] > tall_bar[1] + 1
            or tall_bounds[0] < 0
            or tall_bounds[0] + tall_bounds[2] > width
        )
        if tall_covered:
            raise SystemExit(
                "a long result put the crop label under the scrollbar: "
                f"{tall_bounds}, bar {tall_bar}, window {width}x{height}"
            )
        child = app.detail.get_first_child()
        while child is not None:
            nxt = child.get_next_sibling()
            app.detail.remove(child)
            child = nxt
        vadj.set_value(0)
        pump(context, 15)
        app.summary.set_text("No candidate is not a clearance.")
        app._fill_strip(
            {
                "inventory": [
                    {
                        "crop": "crops/a.png",
                        "label": "Possible candidate. Not an identification.",
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
        finished = app.strip.get_first_child().get_last_child()
        finished_bounds = bounds(app.window, finished)
        finished_bar = bounds(app.window, app.strip_scroll.get_hscrollbar())
        finished_covered = (
            finished_bounds is None
            or finished_bar is None
            or finished.get_text() != "Possible candidate. Not an identification."
            or finished_bounds[1] + finished_bounds[3] > finished_bar[1] + 1
            or finished_bounds[0] < 0
            or finished_bounds[0] + finished_bounds[2] > width
        )
        if finished_covered:
            raise SystemExit(
                "the finished crop label is under the scrollbar: "
                f"{finished_bounds}, bar {finished_bar}, window {width}x{height}"
            )
    finally:
        app.do_shutdown()
    print("desktop crops ok")


if __name__ == "__main__":
    main()
