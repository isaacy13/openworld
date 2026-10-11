#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""A file with no local path still opens, and an old file still says that."""

from __future__ import annotations

import importlib.util
import os
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


class RemoteFile:
    """A chooser result that can be read and has no local path."""

    def __init__(self, payload: bytes, name: str, modified_unix: int | None, readable: bool = True) -> None:
        self.payload = payload
        self.name = name
        self.modified_unix = modified_unix
        self.readable = readable

    def get_path(self):
        return None

    def read(self, _cancellable=None):
        if not self.readable:
            raise OSError("unreadable")
        import gi

        gi.require_version("Gtk", "4.0")
        from gi.repository import GLib, Gio

        return Gio.MemoryInputStream.new_from_bytes(GLib.Bytes.new(self.payload))

    def query_info(self, _attributes, _flags, _cancellable):
        import gi

        gi.require_version("Gtk", "4.0")
        from gi.repository import GLib, Gio

        info = Gio.FileInfo()
        info.set_display_name(self.name)
        if self.modified_unix is not None:
            info.set_modification_date_time(GLib.DateTime.new_from_unix_utc(self.modified_unix))
        return info


def main() -> None:
    if not os.environ.get("DISPLAY"):
        raise SystemExit("no display")
    desktop = load_desktop()
    app = desktop.OpenWorld()
    app.register()
    app.activate()
    try:
        app._on_drop(None, RemoteFile(b"", "gone.png", None, readable=False), 0, 0)
        if app.stack.get_visible_child_name() != "choose" or not app.pick_notice.get_visible():
            raise SystemExit("an unreadable file with no local path left the choose page")
        if app.pick_notice.get_text() != "The file could not be read. Refusing.":
            raise SystemExit(f"unexpected refusal: {app.pick_notice.get_text()!r}")
        app._on_drop(None, RemoteFile(b"png-bytes", "photos/remote.png", 0), 0, 0)
        if app.stack.get_visible_child_name() != "device" or app.file_label.get_text() != "remote.png":
            raise SystemExit(
                f"a file with no local path stayed off the page: {app.file_label.get_text()!r} {app.stack.get_visible_child_name()!r}"
            )
        if app.warn_label.get_text() != "This file is older than about 30 days.":
            raise SystemExit(f"an old file with no local path lost its age: {app.warn_label.get_text()!r}")
        if app.pick_notice.get_visible():
            raise SystemExit("a file that opened left the refusal on the page")
        app._on_drop(None, RemoteFile(b"png-bytes", "posters", 0), 0, 0)
        pack = app.work / "posters"
        if app.file_label.get_text() != "posters" or app.stack.get_visible_child_name() != "device":
            raise SystemExit(
                f"a file named posters stayed off the page: {app.file_label.get_text()!r} {app.stack.get_visible_child_name()!r}"
            )
        if app.input_path is None or Path(app.input_path) == pack or pack.is_file():
            raise SystemExit(f"a file named posters occupied the poster pack: {app.input_path}")
        app._on_drop(None, RemoteFile(b"second-posters", "posters", 0), 0, 0)
        opened = Path(app.input_path) if app.input_path else None
        if (
            app.file_label.get_text() != "posters"
            or app.stack.get_visible_child_name() != "device"
            or opened is None
            or opened.read_bytes() != b"second-posters"
            or app.work / "imports" not in opened.parents
            or pack.exists()
        ):
            raise SystemExit(
                f"a second file named posters left the import folder: {app.file_label.get_text()!r} {opened}"
            )
        if app.warn_label.get_text() != "This file is older than about 30 days.":
            raise SystemExit(f"a second old file lost its age: {app.warn_label.get_text()!r}")
        local = app.work / "local.png"
        local.write_bytes(b"png")
        app._on_drop(None, desktop.Gio.File.new_for_path(str(local)), 0, 0)
        if app.file_label.get_text() != "local.png" or app.stack.get_visible_child_name() != "device":
            raise SystemExit(f"a local file stayed off the page: {app.file_label.get_text()!r}")
        app._stop_scan()
        app.pick_notice.set_visible(False)
        app._on_drop(None, RemoteFile(b"png-bytes", "later.png", 0), 0, 0)
        if (
            app.file_label.get_text() != "local.png"
            or app.stack.get_visible_child_name() != "device"
            or app.pick_notice.get_visible()
        ):
            raise SystemExit(
                f"a file with no local path reached the closed window: {app.file_label.get_text()!r} {app.pick_notice.get_visible()!r}"
            )
    finally:
        app.do_shutdown()
    print("desktop remote ok")


if __name__ == "__main__":
    main()
