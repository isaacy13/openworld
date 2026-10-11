#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""A large font still reaches the last line of a short window."""

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


def scroll_parent(widget, gtk):
    parent = widget.get_parent()
    while parent is not None and not isinstance(parent, gtk.ScrolledWindow):
        parent = parent.get_parent()
    return parent


def reveal(window, widget, gtk) -> None:
    parent = scroll_parent(widget, gtk)
    if parent is None:
        return
    adjustment = parent.get_vadjustment()
    adjustment.set_value(max(0.0, adjustment.get_upper() - adjustment.get_page_size()))
    from gi.repository import GLib

    context = GLib.MainContext.default()
    pump(context, 8)


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
        app.measured_button.set_active(True)
        app._mark_coverage()
        app._go("size")
        app.window.set_default_size(900, 640)
        pump(context, 16)
        height = app.window.get_height()
        reveal(app.window, app.brief_label, Gtk)
        box = bounds(app.window, app.brief_label)
        on_screen = (
            height <= 660
            and box is not None
            and box[3] > 0
            and box[1] >= -1
            and box[1] + box[3] <= height + 1
            and app.brief_label.get_text() == "A brief face can be missed."
        )
        if not on_screen:
            raise SystemExit(
                "the size page left a short window: "
                f"window {app.window.get_width()}x{height}, brief {box}"
            )
        app.window.set_default_size(760, 700)
        pump(context, 12)
        width = app.window.get_width()
        choice = None

        def find_choice(widget):
            nonlocal choice
            if isinstance(widget, Gtk.Label) and widget.get_text().startswith("Measured. 5 frames"):
                choice = widget
            child = widget.get_first_child()
            while child is not None:
                find_choice(child)
                child = child.get_next_sibling()

        find_choice(app.page_size)
        choice_box = None if choice is None else bounds(app.window, choice)
        choice_lines = 0 if choice is None or choice.get_layout() is None else choice.get_layout().get_line_count()
        wrapped = (
            choice is not None
            and choice_box is not None
            and width <= 780
            and choice_box[0] >= -1
            and choice_box[0] + choice_box[2] <= width + 1
            and choice_lines >= 2
            and not choice.get_layout().is_ellipsized()
        )
        if not wrapped:
            raise SystemExit(
                "the coverage choice left a narrow window: "
                f"window {width}x{app.window.get_height()}, choice {choice_box}, lines {choice_lines}"
            )
        indicator = None
        sibling = app.measured_button.get_first_child()
        while sibling is not None:
            if not isinstance(sibling, Gtk.Label):
                indicator = sibling
            sibling = sibling.get_next_sibling()
        mark = None if indicator is None else bounds(app.window, indicator)
        first_line = 0 if choice_box is None else choice_box[3] / choice_lines
        mark_center = None if mark is None else mark[1] + mark[3] / 2
        beside_first_line = (
            mark is not None
            and choice_box is not None
            and mark_center is not None
            and mark_center <= choice_box[1] + first_line * 0.75
        )
        if not beside_first_line:
            raise SystemExit(
                "the coverage mark left the first line: "
                f"choice {choice_box}, mark {mark}, lines {choice_lines}"
            )
        app.load_bundles()
        app._go("bundle")
        app.window.set_default_size(640, 700)
        pump(context, 12)
        width = app.window.get_width()
        curve = None

        def find_curve(widget):
            nonlocal curve
            if isinstance(widget, Gtk.Label) and "Real FBI photos stay off." in (widget.get_text() or ""):
                curve = widget
            child = widget.get_first_child()
            while child is not None:
                find_curve(child)
                child = child.get_next_sibling()

        find_curve(app.page_bundle)
        curve_box = None if curve is None else bounds(app.window, curve)
        curve_lines = 0 if curve is None or curve.get_layout() is None else curve.get_layout().get_line_count()
        curve_wraps = (
            curve is not None
            and curve_box is not None
            and width <= 680
            and curve_box[0] >= -1
            and curve_box[0] + curve_box[2] <= width + 1
            and curve_lines >= 2
            and not curve.get_layout().is_ellipsized()
        )
        if not curve_wraps:
            raise SystemExit(
                "the bundle curve left a narrow window: "
                f"window {width}x{app.window.get_height()}, curve {curve_box}, lines {curve_lines}"
            )
        app._go("device")
        app.window.set_default_size(900, 640)
        pump(context, 12)
        height = app.window.get_height()
        last = app.page_device.get_last_child()
        reveal(app.window, last, Gtk)
        box = None if last is None else bounds(app.window, last)
        if not (
            height <= 660
            and box is not None
            and box[1] + box[3] <= height + 1
            and last.get_text() == "Fixture posters. Real FBI photos stay off."
        ):
            raise SystemExit(
                "the file page left a short window: "
                f"window {app.window.get_width()}x{height}, last {None if last is None else last.get_text()!r} {box}"
            )
    finally:
        app.do_shutdown()
    print("desktop short window ok")


if __name__ == "__main__":
    main()
