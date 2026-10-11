#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""A large crop stays a thumbnail. A wide or tall file does not grow the picture."""

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


def png(width: int, height: int) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    row = b"\x00" + b"\x80\x40\x20" * width
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(row * height))
        + chunk(b"IEND", b"")
    )


def bounds(window, widget):
    found = widget.compute_bounds(window)
    if not found:
        return None
    rect = found.out_bounds
    return rect.get_x(), rect.get_y(), rect.get_width(), rect.get_height()


def picture_of(column):
    from gi.repository import Gtk

    child = column.get_first_child()
    while child is not None:
        if isinstance(child, Gtk.Picture) or child.__class__.__name__ == "BoundedPicture":
            return child
        # The thumbnail may be a widget that holds the picture.
        nested = child.get_first_child()
        if isinstance(child, Gtk.Widget) and not isinstance(child, Gtk.Label):
            if isinstance(nested, Gtk.Picture) or child.get_size_request().width > 0:
                return child
        child = child.get_next_sibling()
    return column.get_first_child()


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
        pump(context, 20)
        folder = app.work / "crops"
        folder.mkdir(parents=True)
        (folder / "big.png").write_bytes(png(480, 360))
        app.out_dir = app.work
        app.history = ["choose", "results"]
        app._show("results")
        app.summary.set_text("Scanning")
        app.primary.set_label("Scanning")
        app.primary.set_sensitive(False)
        app._add_crop(
            {
                "kind": "face",
                "crop": "crops/big.png",
                "label": "Not compared.",
                "frame_label": "Frame 1.",
            }
        )
        pump(context, 20)
        column = app.strip.get_first_child()
        picture = None if column is None else picture_of(column)
        box = None if picture is None else bounds(app.window, picture)
        width, height = app.window.get_width(), app.window.get_height()
        fits = (
            box is not None
            and box[2] <= 180
            and box[3] <= 130
            and box[2] >= 100
            and box[3] >= 100
            and box[0] >= 0
            and box[1] >= 0
            and box[0] + box[2] <= width + 1
            and box[1] + box[3] <= height + 1
        )
        if not fits:
            raise SystemExit(f"a large crop grew past the thumbnail: {box}, window {width}x{height}")
        app._clear_strip()
        app.summary.set_text("Possible candidate. Not an identification.")
        app._fill_strip(
            {
                "inventory": [
                    {
                        "crop": "crops/big.png",
                        "label": "Not compared.",
                        "frame_label": "Frame 1.",
                    }
                ]
            }
        )
        pump(context, 15)
        finished = picture_of(app.strip.get_first_child())
        finished_box = None if finished is None else bounds(app.window, finished)
        if finished_box is None or finished_box[2] > 180 or finished_box[3] > 130 or finished_box[3] < 100:
            raise SystemExit(f"a finished crop grew past the thumbnail: {finished_box}")
        card = app._candidate_card(
            {
                "wording": "Possible candidate. Not an identification.",
                "crop": "crops/big.png",
                "frame": "crops/big.png",
                "frame_label": "Frame 1.",
                "uncertainty": "Score 0.90.",
                "poster_title": "Fixture subject A",
                "poster_class_label": "Missing",
            }
        )
        app.detail.append(card)
        pump(context, 12)
        from gi.repository import Gtk

        pictures = []

        def walk(widget):
            if isinstance(widget, Gtk.Picture) or (
                not isinstance(widget, Gtk.Label) and widget.get_size_request().height in (112, 120)
            ):
                pictures.append(widget)
            child = widget.get_first_child()
            while child is not None:
                walk(child)
                child = child.get_next_sibling()

        walk(card)
        boxes = [bounds(app.window, widget) for widget in pictures]
        if len(boxes) < 2 or any(item is None or item[2] > 200 or item[3] > 150 for item in boxes):
            raise SystemExit(f"a candidate picture grew past its frame: {boxes}")
        (folder / "wide.png").write_bytes(png(800, 100))
        (folder / "tall.png").write_bytes(png(100, 800))
        app._clear_strip()
        app.summary.set_text("Scanning")
        app._add_crop(
            {
                "kind": "face",
                "crop": "crops/wide.png",
                "label": "Not compared.",
                "frame_label": "Frame 1.",
            }
        )
        pump(context, 12)
        wide = picture_of(app.strip.get_first_child())
        wide_box = None if wide is None else bounds(app.window, wide)
        if (
            wide_box is None
            or wide_box[2] > 180
            or wide_box[3] > 130
            or wide_box[2] < 100
            or wide_box[3] < 100
        ):
            raise SystemExit(f"a wide crop grew past the thumbnail: {wide_box}")
        app.detail.remove(card)
        shaped = app._candidate_card(
            {
                "wording": "Possible candidate. Not an identification.",
                "crop": "crops/wide.png",
                "frame": "crops/tall.png",
                "frame_label": "Frame 1.",
                "uncertainty": "Score 0.90.",
                "poster_title": "Fixture subject A",
                "poster_class_label": "Missing",
            }
        )
        app.detail.append(shaped)
        pump(context, 12)
        shaped_pictures: list = []

        def walk_shaped(widget):
            if isinstance(widget, Gtk.Picture) or (
                not isinstance(widget, Gtk.Label) and widget.get_size_request().height in (112, 120)
            ):
                shaped_pictures.append(widget)
            child = widget.get_first_child()
            while child is not None:
                walk_shaped(child)
                child = child.get_next_sibling()

        walk_shaped(shaped)
        shaped_boxes = [bounds(app.window, widget) for widget in shaped_pictures]
        if len(shaped_boxes) < 2 or any(
            item is None or item[2] > 200 or item[3] > 150 or item[2] < 100 or item[3] < 100
            for item in shaped_boxes
        ):
            raise SystemExit(f"a wide or tall candidate grew past its frame: {shaped_boxes}")
    finally:
        app.do_shutdown()
    print("desktop thumb ok")


if __name__ == "__main__":
    main()
