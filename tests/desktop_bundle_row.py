#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""A large font in a short window still shows a whole bundle row."""

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
    from gi.repository import Gtk

    Gtk.Settings.get_default().set_property("gtk-font-name", "Sans 22")
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


def reveal(window, widget, gtk) -> None:
    parent = widget.get_parent()
    while parent is not None:
        if isinstance(parent, gtk.ScrolledWindow):
            widget_box = bounds(window, widget)
            parent_box = bounds(window, parent)
            if widget_box is not None and parent_box is not None:
                adjustment = parent.get_vadjustment()
                delta = widget_box[1] - parent_box[1]
                adjustment.set_value(
                    max(0.0, min(adjustment.get_upper() - adjustment.get_page_size(), adjustment.get_value() + delta))
                )
        parent = parent.get_parent()


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
        pump(context, 16)
        app.load_bundles()
        app._go("bundle")
        app.window.set_default_size(640, 280)
        for _ in range(30):
            pump(context, 2)
            if not app.stack.get_transition_running():
                break
        pump(context, 8)
        curve = None

        def find_curve(widget):
            nonlocal curve
            # The later row is the one a crushed list hides.
            if isinstance(widget, Gtk.Label) and "Real FBI photos stay off." in (widget.get_text() or ""):
                curve = widget
            child = widget.get_first_child()
            while child is not None:
                find_curve(child)
                child = child.get_next_sibling()

        find_curve(app.page_bundle)
        reveal(app.window, curve, Gtk)
        pump(context, 8)
        reveal(app.window, curve, Gtk)
        pump(context, 6)
        height = app.window.get_height()
        curve_box = None if curve is None else bounds(app.window, curve)
        list_box = bounds(app.window, app.bundle_scroll)
        shown = (
            curve is not None
            and curve_box is not None
            and list_box is not None
            and curve_box[3] > 40
            and list_box[3] + 1 >= curve_box[3]
            and curve_box[1] >= -1
            and curve_box[1] + curve_box[3] <= height + 1
            and curve_box[1] >= list_box[1] - 1
            and curve_box[1] + curve_box[3] <= list_box[1] + list_box[3] + 1
            and curve_box[0] >= -1
            and curve_box[0] + curve_box[2] <= app.window.get_width() + 1
        )
        if not shown:
            raise SystemExit(
                "a bundle row did not fit in a short window: "
                f"window {app.window.get_width()}x{height}, curve {curve_box}, list {list_box}"
            )
    finally:
        app.do_shutdown()
    print("desktop bundle row ok")


if __name__ == "__main__":
    main()
