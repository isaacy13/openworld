#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Opening an FBI page shows the address, and a long address stays inside the dialog."""

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


def walk(widget, kind):
    found = []
    if isinstance(widget, kind):
        found.append(widget)
    child = widget.get_first_child()
    while child is not None:
        found.extend(walk(child, kind))
        child = child.get_next_sibling()
    return found


def dialogs(app, gtk):
    toplevels = gtk.Window.get_toplevels()
    opened = []
    for index in range(toplevels.get_n_items()):
        window = toplevels.get_item(index)
        if window is not app.window:
            opened.append(window)
    return opened


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
        url = "https://www.fbi.gov/wanted/" + ("person-" * 40)
        app.confirm_leave(url)
        pump(context, 30)
        opened = dialogs(app, Gtk)
        if len(opened) != 1:
            raise SystemExit(f"Open FBI page did not show one dialog: {len(opened)}")
        dialog = opened[0]
        labels = walk(dialog, Gtk.Label)
        texts = [label.get_text() for label in labels]
        buttons = [button.get_label() for button in walk(dialog, Gtk.Button)]
        address = next((label for label in labels if label.get_text() == url), None)
        width = dialog.get_width()
        height = dialog.get_height()
        box = None if address is None else bounds(dialog, address)
        lines = 0 if address is None or address.get_layout() is None else address.get_layout().get_line_count()
        inside = (
            address is not None
            and box is not None
            and box[2] > 0
            and box[3] > 0
            and box[0] >= -1
            and box[1] >= -1
            and box[0] + box[2] <= width + 1
            and box[1] + box[3] <= height + 1
            and lines > 1
            and "You are leaving OpenWorld." in texts
            and "Stay" in buttons
            and "Open" in buttons
        )
        if not inside:
            raise SystemExit(
                "the FBI address left the dialog: "
                f"box {box}, lines {lines}, dialog {width}x{height}, buttons {buttons}, texts {texts[:4]}"
            )
        stay = next(button for button in walk(dialog, Gtk.Button) if button.get_label() == "Stay")
        stay.activate()
        pump(context, 15)
        if dialogs(app, Gtk):
            raise SystemExit("Stay left the FBI dialog on the screen")
        app.confirm_leave("https://www.fbi.gov.evil.com/wanted")
        pump(context, 20)
        refused = dialogs(app, Gtk)
        if len(refused) != 1:
            raise SystemExit(f"a lookalike host did not show one dialog: {len(refused)}")
        refusal = refused[0]
        refusal_texts = [label.get_text() for label in walk(refusal, Gtk.Label)]
        refusal_buttons = [button.get_label() for button in walk(refusal, Gtk.Button)]
        if (
            "OpenWorld only opens an FBI page." not in refusal_texts
            or "Open" in refusal_buttons
            or url in refusal_texts
        ):
            raise SystemExit(f"a lookalike host was offered: {refusal_texts} {refusal_buttons}")
    finally:
        app.do_shutdown()
    print("desktop leave ok")


if __name__ == "__main__":
    main()
