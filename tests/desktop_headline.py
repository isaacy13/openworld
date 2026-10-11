#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""A short window still shows a full line of the result headline."""

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
        app.summary.set_text("Possible candidate. Not an identification.")
        app._show("results")
        app.primary.set_label("Choose another file")
        app.primary.set_visible(True)
        app.delete_button.set_visible(True)
        app.window.set_default_size(700, 140)
        for _ in range(30):
            pump(context, 2)
            if not app.stack.get_transition_running():
                break
        pump(context, 10)
        layout = app.summary.get_layout()
        line = 0 if layout is None or layout.get_line_count() < 1 else layout.get_line(0).get_pixel_extents()[1].height
        height = app.window.get_height()
        scroll = bounds(app.window, app.results_scroll)
        summary = bounds(app.window, app.summary)
        delete = bounds(app.window, app.delete_button)
        shown = (
            line > 40
            and scroll is not None
            and summary is not None
            and delete is not None
            and scroll[3] + 1 >= line
            and summary[1] >= scroll[1] - 1
            and summary[1] + line <= scroll[1] + scroll[3] + 1
            and delete[1] >= 0
            and delete[1] + delete[3] <= height + 1
            and app.summary.get_text() == "Possible candidate. Not an identification."
        )
        if not shown:
            raise SystemExit(
                "the result headline did not fit above Delete: "
                f"window {app.window.get_width()}x{height}, line {line}, "
                f"summary {summary}, scroll {scroll}, delete {delete}"
            )
    finally:
        app.do_shutdown()
    print("desktop headline ok")


if __name__ == "__main__":
    main()
