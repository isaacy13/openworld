#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""The window title stays whole beside Back and the widest header button."""

from __future__ import annotations

import importlib.util
import os
import time
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
    from gi.repository import GLib, Gtk

    context = GLib.MainContext.default()
    try:
        pump(context, 20)
        app.window.set_default_size(200, 200)
        app.history = ["choose", "device", "results"]
        for name in ("device", "results"):
            app._show(name)
            pump(context, 15)
            title = find_label(app.window.get_titlebar(), "OpenWorld")
            layout = None if title is None else title.get_layout()
            title_box = None if title is None else bounds(app.window, title)
            back_box = bounds(app.window, app.back)
            primary_box = bounds(app.window, app.primary)
            width = app.window.get_width()
            natural = 0 if title is None else title.measure(Gtk.Orientation.HORIZONTAL, -1).natural
            ellipsized = layout is not None and layout.is_ellipsized()
            separated = (
                title_box is not None
                and back_box is not None
                and primary_box is not None
                and title_box[2] + 1 >= natural
                and back_box[0] + back_box[2] <= title_box[0] + 1
                and title_box[0] + title_box[2] <= primary_box[0] + 1
                and primary_box[0] + primary_box[2] <= width + 1
            )
            whole = title is not None and layout is not None and not ellipsized and separated and app.primary.get_visible()
            if not whole:
                raise SystemExit(
                    "the window title was cut off: "
                    f"page {name}, natural {natural}, ellipsized {ellipsized}, "
                    f"title {title_box}, back {back_box}, primary {primary_box} {app.primary.get_label()!r}, "
                    f"window {width}x{app.window.get_height()}"
                )
    finally:
        app.do_shutdown()
    print("desktop title ok")


if __name__ == "__main__":
    main()
