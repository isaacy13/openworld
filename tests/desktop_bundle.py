#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""The selected bundle stays marked when that row is toggled."""

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
        pump(context)
        app.load_bundles()
        app._go("bundle")
        pump(context)
        fast = app._bundle_row("fast")
        accurate = app._bundle_row("accurate")
        if fast is None or accurate is None:
            raise SystemExit("the bundle page did not list Fast and Accurate")
        if app.bundle_id != "fast" or fast.bundle_name_label.get_text() != "Fast. Selected.":
            raise SystemExit(
                f"Fast did not start marked: {app.bundle_id!r} {fast.bundle_name_label.get_text()!r}"
            )
        # Ctrl+Space sends toggle-cursor-row. That press must leave Fast marked.
        fast.grab_focus()
        pump(context)
        app.bundle_list.emit("toggle-cursor-row")
        pump(context)
        selected = app.bundle_list.get_selected_row()
        if (
            selected is None
            or selected.bundle_id != "fast"
            or app.bundle_id != "fast"
            or fast.bundle_name_label.get_text() != "Fast. Selected."
        ):
            raise SystemExit(
                "pressing the selected bundle cleared it: "
                f"{None if selected is None else selected.bundle_id!r} "
                f"{app.bundle_id!r} {fast.bundle_name_label.get_text()!r}"
            )
        app.bundle_list.emit("move-cursor", Gtk.MovementStep.DISPLAY_LINES, 1, False, False)
        pump(context)
        selected = app.bundle_list.get_selected_row()
        if (
            selected is None
            or selected.bundle_id != "accurate"
            or app.bundle_id != "accurate"
            or accurate.bundle_name_label.get_text() != "Accurate. Selected."
            or fast.bundle_name_label.get_text() != "Fast"
        ):
            raise SystemExit(
                "the next bundle did not become the one that will run: "
                f"{None if selected is None else selected.bundle_id!r} "
                f"{app.bundle_id!r} {accurate.bundle_name_label.get_text()!r}"
            )
    finally:
        app.do_shutdown()
    print("desktop bundle ok")


if __name__ == "__main__":
    main()
