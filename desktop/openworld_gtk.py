#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Desktop window for OpenWorld. The scan stays in the Rust library."""

from __future__ import annotations

import atexit
import json
import os
import shutil
import signal
import stat
import subprocess
import tempfile
import threading
import time
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("GdkPixbuf", "2.0")
gi.require_version("Pango", "1.0")
from gi.repository import Gdk, GdkPixbuf, Gio, GLib, Gtk, Pango


PHRASES = {
    "possible": "Possible candidate. Not an identification.",
    "not_compared": "Not compared.",
    "incomplete": "Incomplete.",
    "clearance": "No candidate is not a clearance.",
    "leaving": "You are leaving OpenWorld.",
    "brief": "A brief face can be missed.",
    "on_device": "This file stays on this device.",
    "old": "This file is older than about 30 days.",
}


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def find_bin() -> str:
    env = os.environ.get("OPENWORLD_BIN")
    if env:
        return env
    for name in ("release", "debug"):
        candidate = repo_root() / "core" / "target" / name / "openworld"
        if candidate.is_file():
            return str(candidate)
    return "openworld"


def find_bundles() -> str:
    return os.environ.get("OPENWORLD_BUNDLES", str(repo_root() / "bundles"))


def bounded_picture(path: str, width: int, height: int) -> Gtk.Picture:
    """A crop thumbnail. A wide or tall file stays inside this box."""
    try:
        fitted = GdkPixbuf.Pixbuf.new_from_file_at_scale(path, width, height, True)
        canvas = GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB, True, 8, width, height)
        canvas.fill(0)
        source = fitted if fitted.get_has_alpha() else fitted.add_alpha(False, 0, 0, 0)
        fw = min(source.get_width(), width)
        fh = min(source.get_height(), height)
        x = max(0, (width - fw) // 2)
        y = max(0, (height - fh) // 2)
        source.composite(
            canvas,
            x,
            y,
            fw,
            fh,
            float(x),
            float(y),
            1.0,
            1.0,
            GdkPixbuf.InterpType.NEAREST,
            255,
        )
        picture = Gtk.Picture.new_for_paintable(Gdk.Texture.new_for_pixbuf(canvas))
    except GLib.Error:
        picture = Gtk.Picture.new_for_filename(path)
    picture.set_size_request(width, height)
    picture.set_hexpand(False)
    picture.set_vexpand(False)
    picture.set_halign(Gtk.Align.CENTER)
    picture.set_valign(Gtk.Align.CENTER)
    picture.set_can_shrink(False)
    picture.set_content_fit(Gtk.ContentFit.CONTAIN)
    return picture


def filter_rules(item: Gtk.FileFilter) -> list[str]:
    """The patterns and MIME types on a file dialog filter, in the order they were added."""
    rules = item.to_gvariant().get_child_value(1)
    values: list[str] = []
    for index in range(rules.n_children()):
        values.append(rules.get_child_value(index).get_child_value(1).get_string())
    return values


def drain_scan_pipes(proc: subprocess.Popen[str], on_line) -> str:
    """Read the report and the crop lines together.

    The report is larger than a pipe buffer on a long file. Reading the crop
    lines to the end first leaves the screen on Scanning.
    """
    stdout_box: list[str] = []

    def read_stdout() -> None:
        stream = proc.stdout
        if stream is not None:
            stdout_box.append(stream.read())

    reader = threading.Thread(target=read_stdout, daemon=True)
    reader.start()
    try:
        stream = proc.stderr
        if stream is not None:
            for line in stream:
                on_line(line)
    finally:
        reader.join()
        proc.wait()
    return stdout_box[0] if stdout_box else ""


class OpenWorld(Gtk.Application):
    def __init__(self) -> None:
        super().__init__(application_id="app.openworld.desktop")
        self.bin = find_bin()
        self.bundles = find_bundles()
        self.work = Path(tempfile.mkdtemp(prefix="openworld-"))
        atexit.register(self._remove_work)
        self.input_path: str | None = None
        self.bundle_id = "fast"
        self.long_side = "640"
        self.coverage = "complete"
        self.include_missing = True
        self.include_wanted = True
        self.estimate_ok = False
        self.can_analyze = True
        self.rows: list[dict] = []
        self.out_dir: Path | None = None
        self.posters: Path | None = None
        self.scan_thread: threading.Thread | None = None
        self.scan_proc: subprocess.Popen[str] | None = None
        self.closed = False
        self._scan_lock = threading.Lock()

    def do_activate(self) -> None:
        self.window = Gtk.ApplicationWindow(application=self, title="OpenWorld")
        self.window.set_default_size(980, 860)
        self._css()
        header = Gtk.HeaderBar()
        # The default title shrinks to a few characters, so "OpenWorld" is cut off
        # beside Continue and Choose another file.
        title = Gtk.Label(label="OpenWorld")
        title.set_ellipsize(Pango.EllipsizeMode.NONE)
        header.set_title_widget(title)
        self.back = Gtk.Button(label="Back")
        self.back.connect("clicked", lambda *_: self.go_back())
        header.pack_start(self.back)
        self.primary = Gtk.Button(label="Continue")
        self.primary.add_css_class("suggested-action")
        self.primary.connect("clicked", lambda *_: self.on_primary())
        header.pack_end(self.primary)
        self.window.set_titlebar(header)

        self.stack = Gtk.Stack()
        self.stack.set_transition_type(Gtk.StackTransitionType.SLIDE_LEFT_RIGHT)
        self.stack.set_margin_top(8)
        self.stack.set_margin_bottom(18)
        self.stack.set_margin_start(24)
        self.stack.set_margin_end(24)
        self.pick_notice = Gtk.Label(xalign=0, wrap=True)
        self.pick_notice.add_css_class("warn")
        self.pick_notice.set_visible(False)
        self.pick_notice.set_margin_top(12)
        self.pick_notice.set_margin_start(24)
        self.pick_notice.set_margin_end(24)
        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        root.append(self.pick_notice)
        root.append(self.stack)
        self.window.set_child(root)

        self.page_choose = self._choose_page()
        self.page_device = self._device_page()
        self.page_bundle = self._bundle_page()
        self.page_size = self._size_page()
        self.page_estimate = self._estimate_page()
        self.page_results = self._results_page()
        for name, page in (
            ("choose", self.page_choose),
            ("device", self.page_device),
            ("bundle", self.page_bundle),
            ("size", self.page_size),
            ("estimate", self.page_estimate),
            ("results", self.page_results),
        ):
            # Bundle and results already scroll. The other pages scroll too, so a
            # large font can reach the last line in a short window.
            child = page if name in ("bundle", "results") else self._page_scroll(page)
            self.stack.add_named(child, name)
        self.history = ["choose"]
        self._show("choose")

        drop = Gtk.DropTarget.new(Gio.File, Gdk.DragAction.COPY)
        drop.connect("drop", self._on_drop)
        self.window.add_controller(drop)
        self.window.present()
        if os.environ.get("OPENWORLD_EXERCISE"):
            GLib.idle_add(self._exercise_guard)
        elif os.environ.get("OPENWORLD_INPUT"):
            GLib.idle_add(self._open_env_input)

    def _css(self) -> None:
        css = Gtk.CssProvider()
        css.load_from_data(
            b"""
            .title { font-size: 22px; font-weight: 700; }
            .section { font-size: 16px; font-weight: 600; }
            .dim { opacity: 0.72; }
            .warn { color: #8a5a00; }
            """
        )
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

    def _choose_page(self) -> Gtk.Widget:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        title = Gtk.Label(label="Choose a photo or video", xalign=0)
        title.add_css_class("title")
        hint = Gtk.Label(
            label="Import a file you already have. Drop it here, or use Choose File. There is no camera.",
            xalign=0,
            wrap=True,
        )
        self.choose_hint = hint
        hint.add_css_class("dim")
        button = Gtk.Button(label="Choose File")
        button.set_halign(Gtk.Align.START)
        button.connect("clicked", lambda *_: self.pick_file())
        box.append(title)
        box.append(hint)
        box.append(button)
        return box

    def _device_page(self) -> Gtk.Widget:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label=PHRASES["on_device"], xalign=0)
        title.add_css_class("title")
        self.file_label = Gtk.Label(xalign=0, wrap=True)
        self.file_label.set_wrap_mode(Pango.WrapMode.WORD_CHAR)
        self.file_label.set_hexpand(True)
        self.file_label.add_css_class("section")
        self.warn_label = Gtk.Label(xalign=0, wrap=True)
        self.warn_label.add_css_class("warn")
        self.warn_label.set_visible(False)
        box.append(title)
        box.append(self.file_label)
        box.append(self.warn_label)
        for line in (
            "Nothing is uploaded.",
            "Nobody is enrolled.",
            "OpenWorld does not train on this file.",
            "OpenWorld does not contact an agency.",
            "A candidate is not an identification.",
            PHRASES["clearance"],
            "This file is not authenticated.",
            "On-device does not mean the file is real.",
            "Fixture posters. Real FBI photos stay off.",
        ):
            label = Gtk.Label(label=line, xalign=0, wrap=True)
            box.append(label)
        return box

    def _bundle_page(self) -> Gtk.Widget:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.bundle_title = Gtk.Label(label="Model bundle", xalign=0, wrap=True)
        self.bundle_title.add_css_class("title")
        self.bundle_hint = Gtk.Label(
            label="Scores are not comparable across bundles. Results name the bundle you pick.",
            xalign=0,
            wrap=True,
        )
        self.bundle_hint.add_css_class("dim")
        self.bundle_list = Gtk.ListBox()
        self.bundle_list.set_selection_mode(Gtk.SelectionMode.BROWSE)
        self.bundle_list.connect("row-selected", self._on_bundle_row)
        scroll = Gtk.ScrolledWindow()
        scroll.set_child(self.bundle_list)
        scroll.set_vexpand(True)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        self.bundle_scroll = scroll
        box.append(self.bundle_title)
        box.append(self.bundle_hint)
        box.append(scroll)
        return box

    def _size_page(self) -> Gtk.Widget:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Detection size", xalign=0)
        title.add_css_class("title")
        hint = Gtk.Label(
            label=(
                "Smaller frames are a resize of each decoded frame in memory. "
                "A face under 64 px on that image is left out. "
                "Evidence crops come from the original frame. "
                'If that crop is under 112 px on the short side, the label is "Not compared." '
                "Full resolution is slower."
            ),
            xalign=0,
            wrap=True,
        )
        self.size_hint = hint
        hint.add_css_class("dim")
        box.append(title)
        box.append(hint)
        self.size_buttons = {}
        group = None
        for value, label in (
            ("320", "320 px on the long side"),
            ("480", "480 px on the long side"),
            ("640", "640 px on the long side"),
            ("full", "Full resolution"),
        ):
            button = Gtk.CheckButton(label=label)
            button.base_label = label
            self._wrap_choice(button)
            if group is None:
                group = button
            else:
                button.set_group(group)
            if value == "640":
                button.set_active(True)
            button.connect("toggled", self._on_size, value)
            self.size_buttons[value] = button
            box.append(button)
        self._mark_sizes()
        cover = Gtk.Label(label="Coverage", xalign=0)
        cover.add_css_class("section")
        cover.set_margin_top(12)
        box.append(cover)
        self.complete_button = Gtk.CheckButton(label="Complete. Every decoded frame.")
        self.complete_button.base_label = "Complete. Every decoded frame."
        self._wrap_choice(self.complete_button)
        self.measured_button = Gtk.CheckButton(label="Measured. 5 frames a second, plus the tracker.")
        self.measured_button.base_label = "Measured. 5 frames a second, plus the tracker."
        self._wrap_choice(self.measured_button)
        self.measured_button.set_group(self.complete_button)
        self.complete_button.set_active(True)
        self.complete_button.connect("toggled", self._on_coverage, "complete")
        self.measured_button.connect("toggled", self._on_coverage, "measured")
        self._mark_coverage()
        self.brief_label = Gtk.Label(label=PHRASES["brief"], xalign=0)
        self.brief_label.set_visible(False)
        box.append(self.complete_button)
        box.append(self.measured_button)
        box.append(self.brief_label)
        return box

    def _estimate_page(self) -> Gtk.Widget:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.estimate_title = Gtk.Label(label="Estimate", xalign=0, wrap=True)
        self.estimate_title.add_css_class("title")
        self.estimate_warn = Gtk.Label(xalign=0, wrap=True)
        self.estimate_warn.add_css_class("warn")
        self.estimate_warn.set_visible(False)
        self.estimate_body = Gtk.Label(xalign=0, wrap=True)
        self.missing_button = Gtk.CheckButton(label="Missing")
        self.wanted_button = Gtk.CheckButton(label="Wanted")
        self.missing_button.set_active(True)
        self.wanted_button.set_active(True)
        self.missing_button.connect("toggled", self._on_class)
        self.wanted_button.connect("toggled", self._on_class)
        self.class_label = Gtk.Label(label="Missing and wanted.", xalign=0, wrap=True)
        self.estimate_delete = Gtk.Label(xalign=0, wrap=True)
        self.estimate_delete.add_css_class("warn")
        self.estimate_delete.set_visible(False)
        box.append(self.estimate_title)
        box.append(self.estimate_warn)
        box.append(self.estimate_body)
        box.append(self.class_label)
        box.append(self.missing_button)
        box.append(self.wanted_button)
        box.append(self.estimate_delete)
        return box

    def _page_scroll(self, child: Gtk.Widget) -> Gtk.ScrolledWindow:
        """The page scrolls. The window can be shorter than the page."""
        scroll = Gtk.ScrolledWindow()
        scroll.set_child(child)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_vexpand(True)
        scroll.set_propagate_natural_height(False)
        scroll.set_overlay_scrolling(False)
        return scroll

    def _results_page(self) -> Gtk.Widget:
        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.summary = Gtk.Label(xalign=0, wrap=True)
        self.summary.add_css_class("title")
        self.reason = Gtk.Label(xalign=0, wrap=True)
        self.reason.add_css_class("warn")
        self.reason.set_visible(False)
        self.context_note = Gtk.Label(xalign=0, wrap=True)
        self.context_note.set_visible(False)
        self.warning_note = Gtk.Label(xalign=0, wrap=True)
        self.warning_note.add_css_class("warn")
        self.warning_note.set_visible(False)
        self.result_note = Gtk.Label(xalign=0, wrap=True)
        self.result_note.add_css_class("dim")
        self.result_note.set_visible(False)
        self.strip_heading = Gtk.Label(label="Crops from this file.", xalign=0)
        self.strip_heading.add_css_class("section")
        self.strip_heading.set_visible(False)
        self.strip = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.strip_scroll = Gtk.ScrolledWindow()
        self.strip_scroll.set_child(self.strip)
        self.strip_scroll.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.NEVER)
        self.strip_scroll.set_propagate_natural_height(True)
        self.strip_scroll.set_overlay_scrolling(False)
        self.strip_scroll.set_hexpand(True)
        _minimum, bar_height, _min_baseline, _nat_baseline = self.strip_scroll.get_hscrollbar().measure(
            Gtk.Orientation.VERTICAL, -1
        )
        # The scrollbar sits on the bottom of this row. The margin keeps the crop label above it.
        self.strip.set_margin_bottom(max(bar_height, 16))
        self.detail = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        scroll = Gtk.ScrolledWindow()
        self.results_scroll = scroll
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_overlay_scrolling(False)
        scroll.set_vexpand(True)
        inner = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        inner.append(self.summary)
        inner.append(self.reason)
        inner.append(self.context_note)
        inner.append(self.warning_note)
        inner.append(self.detail)
        inner.append(self.strip_heading)
        inner.append(self.strip_scroll)
        inner.append(self.result_note)
        scroll.set_child(inner)
        outer.append(scroll)
        self.delete_button = Gtk.Button(label="Delete")
        self.delete_button.set_halign(Gtk.Align.START)
        self.delete_button.set_visible(False)
        self.delete_button.connect("clicked", lambda *_: self.delete_result())
        self.delete_notice = Gtk.Label(xalign=0, wrap=True)
        self.delete_notice.add_css_class("warn")
        self.delete_notice.set_visible(False)
        outer.append(self.delete_button)
        outer.append(self.delete_notice)
        return outer

    def file_dialog(self) -> Gtk.FileDialog:
        """Choose File opens on photos and video. Every file stays available in that dialog."""
        dialog = Gtk.FileDialog(title="Choose a photo or video")
        filters = Gio.ListStore.new(Gtk.FileFilter)
        photos = Gtk.FileFilter()
        photos.set_name("Photos and video")
        photos.add_mime_type("image/*")
        photos.add_mime_type("video/*")
        everything = Gtk.FileFilter()
        everything.set_name("All files")
        everything.add_pattern("*")
        filters.append(photos)
        filters.append(everything)
        dialog.set_filters(filters)
        dialog.set_default_filter(photos)
        return dialog

    def pick_file(self) -> None:
        if self.closed:
            return
        self.file_dialog().open(self.window, None, self._file_chosen)

    def _file_chosen(self, dialog: Gtk.FileDialog, result: Gio.AsyncResult) -> None:
        try:
            chosen = dialog.open_finish(result)
        except GLib.Error:
            return
        if chosen is None:
            return
        self.take_file(chosen)

    def _on_drop(self, _target, value, _x, _y) -> bool:
        self.take_file(value)
        return True

    def take_file(self, chosen) -> None:
        """Open a chooser result. A file with no local path is copied onto this device."""
        if self.closed:
            return
        path = None
        display = None
        if chosen is not None:
            getter = getattr(chosen, "get_path", None)
            path = getter() if getter is not None else None
            if not path:
                copied = self._copy_choice(chosen)
                if copied is not None:
                    path, display = copied
        if path:
            self.choose_file(path, display)
        else:
            self._refuse_pick()

    def _copy_choice(self, chosen) -> tuple[str, str] | None:
        try:
            stream = chosen.read(None)
        except Exception:
            return None
        name = "file"
        modified = None
        try:
            info = chosen.query_info(
                "standard::display-name,time::modified",
                Gio.FileQueryInfoFlags.NONE,
                None,
            )
            display = info.get_display_name()
            if display:
                name = Path(display).name
            when = info.get_modification_date_time()
            if when is not None:
                modified = when.to_unix()
        except Exception:
            base = getattr(chosen, "get_basename", lambda: None)()
            if base:
                name = Path(base).name
        if not name or name in (".", ".."):
            name = "file"
        # The scan writes the poster pack at work/posters. Every copy stays in imports.
        folder = self.work / "imports"
        folder.mkdir(exist_ok=True)
        dest = folder / name
        stem = Path(name).stem
        suffix = Path(name).suffix
        # A second file with the same name stays in this folder. The screen keeps the name the person picked.
        placed = None
        for _ in range(8):
            if not dest.exists():
                placed = dest
                break
            dest = folder / f"{stem}-{time.time_ns()}{suffix}"
        if placed is None:
            return None
        dest = placed
        try:
            out = Gio.File.new_for_path(str(dest)).replace(None, False, Gio.FileCreateFlags.NONE, None)
            out.splice(
                stream,
                Gio.OutputStreamSpliceFlags.CLOSE_SOURCE | Gio.OutputStreamSpliceFlags.CLOSE_TARGET,
                None,
            )
        except Exception:
            dest.unlink(missing_ok=True)
            return None
        if modified is not None:
            os.utime(dest, (modified, modified))
        return str(dest), name

    def _refuse_pick(self) -> None:
        self.pick_notice.set_text("The file could not be read. Refusing.")
        self.pick_notice.set_visible(True)

    def choose_file(self, path: str, display_name: str | None = None) -> None:
        if self.closed:
            return
        if self.scan_thread is not None and self.scan_thread.is_alive():
            return
        try:
            info = os.stat(path)
        except OSError:
            self._refuse_pick()
            return
        if not stat.S_ISREG(info.st_mode) or not os.access(path, os.R_OK):
            self._refuse_pick()
            return
        if self.pick_notice.get_text() == "The file could not be read. Refusing.":
            self.pick_notice.set_visible(False)
        if not self.release_result():
            if self.stack.get_visible_child_name() != "results":
                self.pick_notice.set_text(self.delete_notice.get_text() or "The result could not be deleted.")
                self.pick_notice.set_visible(True)
            self.estimate_delete.set_text("")
            self.estimate_delete.set_visible(False)
            return
        self.pick_notice.set_visible(False)
        self.estimate_delete.set_text("")
        self.estimate_delete.set_visible(False)
        self.input_path = path
        self.file_label.set_text(display_name or Path(path).name)
        age = time.time() - os.stat(path).st_mtime
        if age > 30 * 24 * 3600:
            self._set_line(self.warn_label, PHRASES["old"])
            self.estimate_warn.set_text(PHRASES["old"])
            self.estimate_warn.set_visible(True)
        else:
            self._set_line(self.warn_label, "")
            self.estimate_warn.set_text("")
            self.estimate_warn.set_visible(False)
        self.can_analyze = True
        self.history = ["choose"]
        self._go("device")

    def _set_bundle_headline(self, message: str | None) -> None:
        if message:
            self.bundle_title.set_text(message)
            self.bundle_title.add_css_class("warn")
            self.bundle_hint.set_visible(False)
            self.bundle_scroll.set_visible(False)
            return
        self.bundle_title.set_text("Model bundle")
        self.bundle_title.remove_css_class("warn")
        self.bundle_hint.set_visible(True)
        self.bundle_scroll.set_visible(True)

    def load_bundles(self) -> None:
        payload = self._run_json(
            ["--json", "--bundles", self.bundles, "bundles"],
            "The bundle catalog could not be read. Refusing.",
        )
        if payload.get("status") == "refused":
            self.rows = []
            message = payload.get("message") or "The bundle catalog could not be read. Refusing."
            self._set_bundle_headline(message)
        else:
            self.rows = payload.get("bundles", [])
            if self.rows:
                self._set_bundle_headline(None)
            else:
                self._set_bundle_headline("The scan program is not on this device. Refusing.")
        previous = self.bundle_id
        while True:
            row = self.bundle_list.get_row_at_index(0)
            if row is None:
                break
            self.bundle_list.remove(row)
        kept = None
        selected = None
        for item in self.rows:
            row = Gtk.ListBoxRow()
            row.bundle_id = item["id"]
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
            box.set_margin_top(8)
            box.set_margin_bottom(8)
            box.set_margin_start(10)
            box.set_margin_end(10)
            name = Gtk.Label(label=item["name"], xalign=0)
            name.add_css_class("section")
            row.bundle_name = item["name"]
            row.bundle_name_label = name
            best = Gtk.Label(label=item["best_for"], xalign=0)
            curve = Gtk.Label(label=item["curve_line"], xalign=0)
            curve.add_css_class("dim")
            box.append(name)
            box.append(best)
            box.append(curve)
            row.set_child(box)
            self.bundle_list.append(row)
            if item.get("id") == previous:
                kept = row
            if item.get("preselected"):
                selected = row
        chosen = kept or selected
        if chosen is not None:
            self.bundle_list.select_row(chosen)
        self._mark_bundle_rows()

    def _mark_bundle_rows(self) -> None:
        index = 0
        while True:
            row = self.bundle_list.get_row_at_index(index)
            if row is None:
                return
            label = getattr(row, "bundle_name_label", None)
            title = getattr(row, "bundle_name", "")
            if label is not None:
                label.set_text(f"{title}. Selected." if getattr(row, "bundle_id", None) == self.bundle_id else title)
            index += 1

    def _bundle_row(self, bundle_id: str):
        index = 0
        while True:
            row = self.bundle_list.get_row_at_index(index)
            if row is None:
                return None
            if getattr(row, "bundle_id", None) == bundle_id:
                return row
            index += 1

    def _on_bundle_row(self, _list, row) -> None:
        if row is not None:
            self.bundle_id = row.bundle_id
            self._mark_bundle_rows()

    def _selected_label(self, label: str, selected: bool) -> str:
        if not selected:
            return label
        if label.endswith("."):
            return f"{label} Selected."
        return f"{label}. Selected."

    def _wrap_choice(self, button: Gtk.CheckButton) -> None:
        """A long choice uses the width it is given. The mark stays on the first line."""
        child = button.get_first_child()
        while child is not None:
            if isinstance(child, Gtk.Label):
                child.set_wrap(True)
                child.set_wrap_mode(Pango.WrapMode.WORD)
                child.set_xalign(0)
                child.set_hexpand(True)
                if getattr(button, "choice_label", None) is not child:
                    button.choice_label = child
                    child.connect("notify::width", lambda *_args, button=button: self._align_choice_mark(button))
            child = child.get_next_sibling()
        self._align_choice_mark(button)

    def _align_choice_mark(self, button: Gtk.CheckButton) -> None:
        icon = None
        label = None
        child = button.get_first_child()
        while child is not None:
            if isinstance(child, Gtk.Label):
                label = child
            else:
                icon = child
            child = child.get_next_sibling()
        if icon is None or label is None:
            return
        layout = label.get_layout()
        if layout is None or layout.get_line_count() < 1:
            return
        _ink, logical = layout.get_line_readonly(0).get_extents()
        line_px = logical.height / Pango.SCALE
        if line_px < 1:
            return
        icon_h = icon.get_allocated_height()
        if icon_h < 1:
            icon_h = 16
        icon.set_valign(Gtk.Align.START)
        icon.set_margin_top(max(0, int(round((line_px - icon_h) / 2.0))))

    def _set_choice_text(self, button: Gtk.CheckButton, text: str) -> None:
        button.set_label(text)
        self._wrap_choice(button)

    def _mark_sizes(self) -> None:
        for button in self.size_buttons.values():
            self._set_choice_text(button, self._selected_label(button.base_label, button.get_active()))

    def _mark_coverage(self) -> None:
        for button in (self.complete_button, self.measured_button):
            self._set_choice_text(button, self._selected_label(button.base_label, button.get_active()))

    def _on_size(self, button: Gtk.CheckButton, value: str) -> None:
        if button.get_active():
            self.long_side = value
        self._mark_sizes()

    def _on_coverage(self, button: Gtk.CheckButton, value: str) -> None:
        if button.get_active():
            self.coverage = value
            self.brief_label.set_visible(value == "measured")
        self._mark_coverage()

    def refresh_estimate(self) -> None:
        if not self.input_path:
            return
        payload = self._run_json(
            [
                "--json",
                "--bundles",
                self.bundles,
                "estimate",
                "--input",
                self.input_path,
                "--bundle",
                self.bundle_id,
                "--long-side",
                self.long_side,
                "--coverage",
                self.coverage,
                "--form-factor",
                "computer",
                "--provider",
                "cpu",
            ]
        )
        if payload.get("status") == "refused":
            message = payload.get("message") or "Refusing."
            self.estimate_title.set_text(message)
            self.estimate_title.add_css_class("warn")
            self.estimate_body.set_text("")
            self.estimate_body.set_visible(False)
            self.estimate_warn.set_visible(False)
            self.estimate_ok = False
            self.can_analyze = False
            self._show_classes(False)
            return
        self.estimate_title.set_text("Estimate")
        self.estimate_title.remove_css_class("warn")
        self.estimate_body.set_visible(True)
        self.estimate_warn.set_visible(self.estimate_warn.get_text() == PHRASES["old"])
        self.estimate_ok = True
        self.can_analyze = True
        self._show_classes(True)
        lines = [payload.get("human", ""), payload.get("caveat", "")]
        if payload.get("device_note"):
            lines.append(payload["device_note"])
        if payload.get("heat_note"):
            lines.append(payload["heat_note"])
        if payload.get("battery_note"):
            lines.append(payload["battery_note"])
        if payload.get("suggest_computer_text"):
            lines.append(payload["suggest_computer_text"])
        lines.append(self.choice_line())
        if self.coverage == "measured":
            lines.append(PHRASES["brief"])
        self.estimate_body.set_text("\n".join(line for line in lines if line))
        self._apply_class_gate()

    def _on_class(self, *_args: object) -> None:
        self.include_missing = self.missing_button.get_active()
        self.include_wanted = self.wanted_button.get_active()
        self._apply_class_gate()
        if self.stack.get_visible_child_name() == "estimate" and self.scan_thread is None:
            self.primary.set_sensitive(self.can_analyze)

    def class_line(self) -> str:
        if self.include_missing and self.include_wanted:
            return "Missing and wanted."
        if self.include_missing:
            return "Missing."
        if self.include_wanted:
            return "Wanted."
        return "Choose missing, wanted, or both."

    def _show_classes(self, visible: bool) -> None:
        self.missing_button.set_visible(visible)
        self.wanted_button.set_visible(visible)
        self.class_label.set_visible(visible)

    def _apply_class_gate(self) -> None:
        self.class_label.set_text(self.class_line())
        if self.estimate_ok:
            self.can_analyze = self.include_missing or self.include_wanted

    def _class_args(self) -> list[str]:
        args: list[str] = []
        if not self.include_missing:
            args.append("--no-missing")
        if not self.include_wanted:
            args.append("--no-wanted")
        return args

    def choice_line(self) -> str:
        name = self.bundle_id
        for item in self.rows:
            if item.get("id") == self.bundle_id and item.get("name"):
                name = item["name"]
                break
        if self.long_side == "full":
            size = "Full resolution"
        else:
            size = f"{self.long_side} px on the long side"
        if self.coverage == "measured":
            cover = "5 frames a second, plus the tracker."
        else:
            cover = "Every decoded frame."
        return f"{name}. {size}. {cover}"

    def start_scan(self) -> None:
        if self.closed:
            return
        if self.scan_thread is not None and self.scan_thread.is_alive():
            return
        # A result that cannot be deleted stays. The estimate keeps that sentence.
        kept_delete = (
            self.pick_notice.get_visible()
            and self.pick_notice.get_text() not in ("", "The file could not be read. Refusing.")
        )
        if not self.release_result():
            notice = self.delete_notice.get_text() or "The result could not be deleted."
            if (
                self.pick_notice.get_visible()
                and self.pick_notice.get_text() == "The file could not be read. Refusing."
            ):
                self.estimate_delete.set_text(notice)
                self.estimate_delete.set_visible(True)
            elif not (self.pick_notice.get_visible() and self.pick_notice.get_text() == notice):
                self.pick_notice.set_text(notice)
                self.pick_notice.set_visible(True)
            return
        if kept_delete:
            self.pick_notice.set_visible(False)
        self.estimate_delete.set_text("")
        self.estimate_delete.set_visible(False)
        self._clear_results()
        self._set_line(self.context_note, "")
        self._set_line(self.result_note, "")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self.warning_note.set_text("")
        self.warning_note.set_visible(False)
        self.summary.set_text("Scanning")
        self.summary.remove_css_class("warn")
        self.delete_notice.set_text("")
        self.delete_notice.set_visible(False)
        self._show_delete(False)
        self._go("results")
        self.primary.set_label("Scanning")
        self.primary.set_sensitive(False)
        self.back.set_sensitive(False)
        shot = os.environ.get("OPENWORLD_SCANNING_SHOT")
        if shot:
            context = GLib.MainContext.default()
            deadline = time.time() + 0.4
            while time.time() < deadline:
                context.iteration(False)
                time.sleep(0.05)
            self._grab(shot)
        self.posters = self.work / "posters"
        # A folder that is not a result stays where it is. The next scan uses a new directory.
        self.out_dir = self.work / f"result-{time.time_ns()}"
        written = self._run_json(
            ["--json", "--bundles", self.bundles, "posters", "write-fixture", "--out", str(self.posters)],
            "The poster pack could not be read. Refusing.",
        )
        if written.get("status") == "refused" or not written.get("id"):
            message = written.get("message") or written.get("summary") or "The poster pack could not be read. Refusing."
            self.out_dir = None
            self._show_report(
                {
                    "status": "refused",
                    "summary": message,
                    "message": message,
                    "disclosure": self._refusal_disclosure(),
                    "candidates": [],
                    "inventory": [],
                }
            )
            return
        out_dir = self.out_dir
        classes = self._class_args()
        self.scan_thread = threading.Thread(target=self._scan_worker, args=(out_dir, classes), daemon=True)
        self.scan_thread.start()

    def _scan_worker(self, out_dir: Path, classes: list[str]) -> None:
        proc = subprocess.Popen(
            [
                self.bin,
                "--json",
                "--bundles",
                self.bundles,
                "scan",
                "--input",
                self.input_path or "",
                "--bundle",
                self.bundle_id,
                "--long-side",
                self.long_side,
                "--coverage",
                self.coverage,
                "--posters",
                str(self.posters),
                "--out",
                str(out_dir),
                *classes,
                "--form-factor",
                "computer",
                "--provider",
                "cpu",
                "--progress",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        if not self._own_scan_process(proc):
            return

        def on_line(line: str) -> None:
            line = line.strip()
            if not line:
                return
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                return
            GLib.idle_add(self._add_crop, event)

        try:
            stdout = drain_scan_pipes(proc, on_line)
            try:
                report = json.loads(stdout)
            except json.JSONDecodeError:
                report = self._unreadable_scan_report()
            GLib.idle_add(self._show_report, report)
        finally:
            if self.scan_proc is proc:
                self.scan_proc = None

    def _add_crop(self, event: dict) -> bool:
        if self.closed:
            return False
        kind = event.get("kind")
        label = event.get("label") or ""
        rel = event.get("crop")
        if kind not in ("face", "plate", "vehicle") or not rel or not label or self.out_dir is None:
            return False
        path = self.out_dir / rel
        if not path.is_file():
            return False
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        picture = bounded_picture(str(path), 112, 112)
        caption = self._crop_caption(label)
        box.append(picture)
        frame = event.get("frame_label") or ""
        if frame:
            box.append(Gtk.Label(label=frame, xalign=0.5))
        box.append(caption)
        self.strip.append(box)
        self.strip_heading.set_visible(True)
        self._reveal_latest_crop(self.strip_scroll.get_hadjustment())
        return False

    def _refusal_disclosure(self) -> list[str]:
        return [
            "Nothing is uploaded.",
            "Nobody is enrolled.",
            "OpenWorld does not train on this file.",
            "OpenWorld does not contact an agency.",
            "A candidate is not an identification.",
            "This file is not authenticated.",
            "On-device does not mean the file is real.",
        ]

    def _unreadable_scan_report(self) -> dict:
        return {
            "status": "refused",
            "summary": "The scan could not be read. Refusing.",
            "message": "The scan could not be read. Refusing.",
            "disclosure": self._refusal_disclosure(),
            "candidates": [],
            "inventory": [],
        }

    def _show_report(self, report: dict) -> bool:
        if self.closed:
            return False
        self.scan_thread = None
        # The unread-file sentence was about a file this scan did not use.
        if self.pick_notice.get_text() == "The file could not be read. Refusing.":
            self.pick_notice.set_visible(False)
        summary = report.get("summary") or ""
        status = report.get("status") or ""
        if not summary:
            if status == "complete":
                summary = PHRASES["possible"] if report.get("candidates") else PHRASES["clearance"]
            elif status == "refused":
                summary = report.get("message") or "Refusing."
            else:
                summary = PHRASES["incomplete"]
        self.summary.set_text(summary)
        if status == "refused" or summary.endswith("Refusing."):
            self.summary.add_css_class("warn")
        else:
            self.summary.remove_css_class("warn")
        reason = report.get("message") or ""
        if status == "incomplete" and reason and reason != summary:
            self.reason.set_text(reason)
            self.reason.set_visible(True)
        else:
            self.reason.set_text("")
            self.reason.set_visible(False)
        context = []
        if report.get("coverage_banner"):
            context.append(report["coverage_banner"])
        if report.get("frames_note"):
            context.append(report["frames_note"])
        if report.get("class_note"):
            context.append(report["class_note"])
        if report.get("bundle_name"):
            context.append(f"Bundle: {report['bundle_name']}")
        if report.get("detection_note"):
            context.append(report["detection_note"])
        if report.get("coverage_note"):
            context.append(report["coverage_note"])
        if report.get("perception_note"):
            context.append(report["perception_note"])
        self._set_line(self.context_note, "\n".join(context))
        warnings = [line for line in (report.get("warnings") or []) if line]
        self.warning_note.set_text("\n".join(warnings))
        self.warning_note.set_visible(bool(warnings))
        self._set_line(self.result_note, "\n".join(report.get("disclosure") or []))
        self.results_scroll.get_vadjustment().set_value(0)
        self._fill_strip(report)
        self.strip_scroll.get_hadjustment().set_value(0)
        strip_parent = self.strip.get_parent()
        if strip_parent is not None:
            strip_parent.queue_allocate()
        for candidate in report.get("candidates") or []:
            self.detail.append(self._candidate_card(candidate))
        self.primary.set_sensitive(True)
        self.primary.set_label("Choose another file")
        self.delete_notice.set_visible(False)
        self._show_delete(self.out_dir is not None and (self.out_dir / "result.json").is_file())
        self.back.set_sensitive(len(self.history) > 1)
        self._exercise_report = report
        return False

    def _reveal_latest_crop(self, _adjustment=None) -> None:
        """The crop that just arrived stays on screen. Scanning stays where it is."""
        if self.summary.get_text() != "Scanning":
            return
        self._strip_reveal_tries = 0
        GLib.idle_add(self._scroll_strip_to_latest)

    def _scroll_strip_to_latest(self) -> bool:
        if self.closed or self.summary.get_text() != "Scanning":
            return False
        adjustment = self.strip_scroll.get_hadjustment()
        page = adjustment.get_page_size()
        if page <= 1:
            self._strip_reveal_tries += 1
            return self._strip_reveal_tries < 8
        target = max(0.0, adjustment.get_upper() - page)
        # A value set while the row is being allocated does not move it.
        adjustment.set_value(0)
        adjustment.set_value(target)
        parent = self.strip.get_parent()
        if parent is not None:
            parent.queue_allocate()
        return False

    def _crop_caption(self, text: str) -> Gtk.Label:
        caption = Gtk.Label(label=text, wrap=True, justify=Gtk.Justification.CENTER)
        caption.set_width_chars(18)
        caption.set_max_width_chars(18)
        _minimum, width, _min_baseline, _nat_baseline = caption.measure(Gtk.Orientation.HORIZONTAL, -1)
        _minimum_height, height, _min_baseline, _nat_baseline = caption.measure(
            Gtk.Orientation.VERTICAL, width
        )
        # A tall result allocates this row's minimum height. That minimum is one
        # line, so the rest of a wrapped label slides under the scrollbar.
        caption.set_size_request(width, height)
        return caption

    def _clear_strip(self) -> None:
        child = self.strip.get_first_child()
        while child is not None:
            nxt = child.get_next_sibling()
            self.strip.remove(child)
            child = nxt

    def _fill_strip(self, report: dict) -> None:
        self._clear_strip()
        self.strip_heading.set_visible(False)
        if self.out_dir is None:
            return
        shown = False
        for item in report.get("inventory") or []:
            rel = item.get("crop")
            if not rel:
                continue
            path = self.out_dir / rel
            if not path.is_file():
                continue
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
            picture = bounded_picture(str(path), 112, 112)
            frame = Gtk.Label(label=item.get("frame_label") or "", xalign=0.5)
            caption = self._crop_caption(item.get("label", ""))
            box.append(picture)
            box.append(frame)
            box.append(caption)
            self.strip.append(box)
            shown = True
        self.strip_heading.set_visible(shown)

    def _labels_under(self, root: Gtk.Widget) -> list[str]:
        labels: list[str] = []
        if isinstance(root, Gtk.Label):
            labels.append(root.get_text() or "")
        child = root.get_first_child()
        while child is not None:
            labels.extend(self._labels_under(child))
            child = child.get_next_sibling()
        return labels

    def _button_labels(self, root: Gtk.Widget) -> list[str]:
        labels: list[str] = []
        if isinstance(root, Gtk.Button):
            labels.append(root.get_label() or "")
        child = root.get_first_child()
        while child is not None:
            labels.extend(self._button_labels(child))
            child = child.get_next_sibling()
        return labels

    def _strip_labels(self) -> list[str]:
        labels: list[str] = []
        child = self.strip.get_first_child()
        while child is not None:
            widget = child.get_first_child()
            while widget is not None:
                if isinstance(widget, Gtk.Label):
                    labels.append(widget.get_text())
                widget = widget.get_next_sibling()
            child = child.get_next_sibling()
        return labels

    def _candidate_card(self, candidate: dict) -> Gtk.Widget:
        frame = Gtk.Frame()
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        box.set_margin_top(8)
        box.set_margin_bottom(8)
        box.set_margin_start(8)
        box.set_margin_end(8)
        wording = Gtk.Label(label=candidate.get("wording", PHRASES["possible"]), xalign=0, wrap=True)
        wording.add_css_class("section")
        box.append(wording)
        images = Gtk.Box(spacing=8)
        for key in ("crop", "frame"):
            rel = candidate.get(key)
            caption = "Crop" if key == "crop" else (candidate.get("frame_label") or "Frame")
            if rel and self.out_dir is not None:
                path = self.out_dir / rel
                if path.is_file():
                    pair = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
                    picture = bounded_picture(str(path), 160, 120)
                    label = Gtk.Label(label=caption, xalign=0)
                    pair.append(picture)
                    pair.append(label)
                    images.append(pair)
        box.append(images)
        uncertainty = Gtk.Label(label=candidate.get("uncertainty", ""), xalign=0, wrap=True)
        title = candidate.get("poster_title", "")
        class_label = candidate.get("poster_class_label", "")
        poster_line = f"{title} ({class_label})" if title and class_label else title or class_label
        box.append(uncertainty)
        if poster_line:
            poster = Gtk.Label(
                label=poster_line,
                xalign=0,
                wrap=True,
            )
            box.append(poster)
        url = candidate.get("fbi_url") or ""
        if url:
            button = Gtk.Button(label="Open FBI page")
            button.set_halign(Gtk.Align.START)
            button.connect("clicked", lambda *_c, url=url: self.confirm_leave(url))
            box.append(button)
        frame.set_child(box)
        return frame

    def _leave_address_height(self) -> int:
        """Room for the address. The title and the buttons stay on the screen."""
        height = 600
        display = self.window.get_display()
        if display is not None:
            monitors = display.get_monitors()
            if monitors is not None and monitors.get_n_items() > 0:
                height = monitors.get_item(0).get_geometry().height
        return max(96, height // 3)

    def leave_decision(self, url: str) -> tuple[str, str | None]:
        payload = self._run_json(
            ["--json", "leave", "--url", url],
            "OpenWorld only opens an FBI page.",
        )
        allowed = payload.get("url") or ""
        message = payload.get("message") or ""
        if message != PHRASES["leaving"] or not allowed:
            return message or "OpenWorld only opens an FBI page.", None
        return message, allowed

    def confirm_leave(self, url: str) -> None:
        if self.closed:
            return
        message, allowed = self.leave_decision(url)
        if allowed is None:
            dialog = Gtk.MessageDialog(
                transient_for=self.window,
                modal=True,
                message_type=Gtk.MessageType.WARNING,
                buttons=Gtk.ButtonsType.CLOSE,
                text=message,
            )
            dialog.connect("response", lambda d, *_: d.destroy())
            dialog.present()
            return
        dialog = Gtk.MessageDialog(
            transient_for=self.window,
            modal=True,
            message_type=Gtk.MessageType.QUESTION,
            buttons=Gtk.ButtonsType.NONE,
            text=message,
        )
        dialog.add_button("Stay", Gtk.ResponseType.CANCEL)
        dialog.add_button("Open", Gtk.ResponseType.ACCEPT)
        # GTK 4 dropped format_secondary_text. A URL has no spaces, so the
        # address wraps by character. A long address scrolls, and Stay and Open
        # stay on the screen.
        address = Gtk.Label(label=allowed, wrap=True, xalign=0, selectable=True)
        address.set_wrap_mode(Pango.WrapMode.WORD_CHAR)
        address.set_max_width_chars(42)
        scroll = Gtk.ScrolledWindow()
        scroll.set_child(address)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_propagate_natural_width(True)
        scroll.set_propagate_natural_height(True)
        scroll.set_max_content_height(self._leave_address_height())
        scroll.set_overlay_scrolling(False)
        dialog.get_message_area().append(scroll)
        dialog.connect("response", self._leave_response, allowed)
        dialog.present()

    def _leave_response(self, dialog: Gtk.MessageDialog, response: int, url: str) -> None:
        dialog.destroy()
        if response == Gtk.ResponseType.ACCEPT and url:
            Gio.AppInfo.launch_default_for_uri(url, None)

    def delete_result(self) -> bool:
        if self.closed:
            return False
        if self.out_dir is None:
            return True
        if not self.out_dir.exists():
            self._mark_deleted()
            return True
        payload = self._run_json(
            ["--json", "delete", "--out", str(self.out_dir)],
            "The result could not be deleted.",
        )
        if not payload.get("deleted"):
            self.delete_notice.set_text(payload.get("message") or "The result could not be deleted.")
            self.delete_notice.set_visible(True)
            return False
        self._mark_deleted()
        return True

    def release_result(self) -> bool:
        """Leave a non-result folder on disk and continue. A real result is deleted."""
        out = self.out_dir
        if out is not None and out.exists() and not (out / "result.json").is_file():
            self.out_dir = None
            self.delete_notice.set_text("")
            self.delete_notice.set_visible(False)
            self.estimate_delete.set_text("")
            self.estimate_delete.set_visible(False)
            self._show_delete(False)
            return True
        return self.delete_result()

    def _set_line(self, label: Gtk.Label, text: str) -> None:
        """A note with nothing to say stays off the page. An empty label still takes a line."""
        label.set_text(text)
        label.set_visible(bool(text))

    def _show_delete(self, visible: bool) -> None:
        self.delete_button.set_visible(visible)
        self.delete_button.set_sensitive(visible)

    def _mark_deleted(self) -> None:
        self._clear_results()
        self._set_line(self.context_note, "")
        self._set_line(self.result_note, "")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self.warning_note.set_text("")
        self.warning_note.set_visible(False)
        self.summary.set_text("Deleted.")
        self.summary.remove_css_class("warn")
        self.out_dir = None
        self.delete_notice.set_text("")
        self.delete_notice.set_visible(False)
        self.estimate_delete.set_text("")
        self.estimate_delete.set_visible(False)
        self._show_delete(False)

    def _remove_work(self) -> None:
        shutil.rmtree(self.work, ignore_errors=True)

    def _own_scan_process(self, proc: subprocess.Popen[str]) -> bool:
        """Keep a scan that started while the window is open. Stop one that starts as it closes."""
        with self._scan_lock:
            if self.closed:
                owned = False
            else:
                self.scan_proc = proc
                owned = True
        if not owned:
            self._end_process(proc)
        return owned

    def _end_process(self, proc: subprocess.Popen[str]) -> None:
        if proc.poll() is not None:
            return
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            proc.terminate()
        try:
            proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                proc.kill()
            proc.wait(timeout=2)

    def _stop_scan(self) -> None:
        """Closing the window stops a scan that is still running, so its folder can be removed."""
        with self._scan_lock:
            self.closed = True
            proc = self.scan_proc
            self.scan_proc = None
        if proc is not None:
            self._end_process(proc)
        thread = self.scan_thread
        if thread is not None and thread.daemon and thread.is_alive() and thread is not threading.current_thread():
            thread.join(timeout=2)

    def do_shutdown(self) -> None:
        self._stop_scan()
        self._remove_work()
        Gtk.Application.do_shutdown(self)

    def _clear_results(self) -> None:
        self._clear_strip()
        self.strip_heading.set_visible(False)
        child = self.detail.get_first_child()
        while child is not None:
            nxt = child.get_next_sibling()
            self.detail.remove(child)
            child = nxt

    def on_primary(self) -> None:
        if self.closed:
            return
        name = self.stack.get_visible_child_name()
        if name == "device":
            self.load_bundles()
            self._go("bundle")
        elif name == "bundle":
            if not self.rows:
                return
            self._go("size")
        elif name == "size":
            self.refresh_estimate()
            self._go("estimate")
        elif name == "estimate":
            self.start_scan()
        elif name == "results":
            self.choose_another()

    def choose_another(self) -> None:
        if self.closed:
            return
        if self.scan_thread is not None and self.scan_thread.is_alive():
            return
        if not self.release_result():
            return
        self.estimate_delete.set_text("")
        self.estimate_delete.set_visible(False)
        self.input_path = None
        self.scan_thread = None
        self._clear_results()
        self.summary.set_text("")
        self._set_line(self.context_note, "")
        self._set_line(self.result_note, "")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self.warning_note.set_text("")
        self.warning_note.set_visible(False)
        self.file_label.set_text("")
        self._set_line(self.warn_label, "")
        self.estimate_warn.set_text("")
        self.estimate_warn.set_visible(False)
        self.coverage = "complete"
        self.complete_button.set_active(True)
        self.include_missing = True
        self.include_wanted = True
        self.missing_button.set_active(True)
        self.wanted_button.set_active(True)
        self.estimate_ok = False
        self.class_label.set_text("Missing and wanted.")
        self.long_side = "640"
        self.size_buttons["640"].set_active(True)
        self.bundle_id = "fast"
        self.can_analyze = True
        self.primary.set_sensitive(True)
        self.history = ["choose"]
        self._show("choose")

    def go_back(self) -> None:
        if self.closed:
            return
        if len(self.history) <= 1:
            return
        self.history.pop()
        self._show(self.history[-1])
        notice = self.delete_notice.get_text()
        if notice and self.pick_notice.get_text() == notice:
            self.pick_notice.set_visible(False)
        self.delete_notice.set_text("")
        self.delete_notice.set_visible(False)
        self.estimate_delete.set_text("")
        self.estimate_delete.set_visible(False)

    def _go(self, name: str) -> None:
        if self.history[-1] != name:
            self.history.append(name)
        self._show(name)

    def _show(self, name: str) -> None:
        self.stack.set_visible_child_name(name)
        scanning = self.scan_thread is not None and self.scan_thread.is_alive()
        self.back.set_sensitive(len(self.history) > 1 and not scanning)
        if scanning:
            self.primary.set_sensitive(False)
        elif name == "estimate":
            self.primary.set_sensitive(self.can_analyze)
        elif name == "bundle":
            self.primary.set_sensitive(bool(self.rows))
        else:
            self.primary.set_sensitive(True)
        labels = {
            "choose": "Choose File",
            "device": "Continue",
            "bundle": "Continue",
            "size": "Continue",
            "estimate": "Analyze",
            "results": "Choose another file",
        }
        self.primary.set_label(labels.get(name, "Continue"))
        refused_estimate = name == "estimate" and not self.estimate_ok
        self.primary.set_visible(name != "choose" and not refused_estimate)

    def _run_json(self, args: list[str], refusal: str = "The scan could not be read. Refusing.") -> dict:
        try:
            proc = subprocess.run([self.bin, *args], check=False, capture_output=True, text=True)
        except OSError:
            message = "The scan program is not on this device. Refusing."
            return {"status": "refused", "summary": message, "message": message}
        text = (proc.stdout or "").strip()
        parsed: dict | None = None
        if text:
            try:
                value = json.loads(text)
            except json.JSONDecodeError:
                value = None
            if isinstance(value, dict):
                parsed = value
        if parsed is None:
            return {"status": "refused", "summary": refusal, "message": refusal}
        return parsed

    def _open_env_input(self) -> bool:
        path = os.environ.get("OPENWORLD_INPUT")
        if path:
            self.choose_file(path)
        return False

    def _exercise_guard(self) -> bool:
        try:
            return self._exercise()
        except Exception as exc:
            import traceback
            self._exercise_fail(traceback.format_exc())
            return False

    def _exercise(self) -> bool:
        path = os.environ.get("OPENWORLD_INPUT")
        if not path:
            self._exercise_fail("missing input")
            return False
        dialog = self.file_dialog()
        filters = dialog.get_filters()
        default = dialog.get_default_filter()
        if (
            dialog.get_title() != "Choose a photo or video"
            or filters is None
            or filters.get_n_items() != 2
            or filters.get_item(0).get_name() != "Photos and video"
            or filter_rules(filters.get_item(0)) != ["image/*", "video/*"]
            or filters.get_item(1).get_name() != "All files"
            or filter_rules(filters.get_item(1)) != ["*"]
            or default is None
            or default.get_name() != "Photos and video"
        ):
            self._exercise_fail(
                f"Choose File did not open on photos and video: {dialog.get_title()!r}"
            )
            return False
        self._show_report(
            {
                "status": "incomplete",
                "summary": "",
                "message": "The file was not fully decoded.",
                "disclosure": ["Nothing is uploaded."],
                "candidates": [],
                "inventory": [],
            }
        )
        if (
            self.summary.get_text() != PHRASES["incomplete"]
            or "warn" in self.summary.get_css_classes()
            or self.reason.get_text() != "The file was not fully decoded."
            or not self.reason.get_visible()
            or "warn" not in self.reason.get_css_classes()
        ):
            self._exercise_fail(f"incomplete reason missing: {self.summary.get_text()!r} {self.reason.get_text()!r}")
            return False
        self._exercise_report = None
        self._clear_results()
        self.summary.set_text("")
        self._set_line(self.context_note, "")
        self._set_line(self.result_note, "")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self._show_report(self._unreadable_scan_report())
        if (
            self.summary.get_text() != "The scan could not be read. Refusing."
            or PHRASES["clearance"] in self.result_note.get_text()
            or "Nothing is uploaded." not in self.result_note.get_text()
        ):
            self._exercise_fail(
                f"an unreadable scan lost the refusal: {self.summary.get_text()!r} {self.result_note.get_text()!r}"
            )
            return False
        if self.delete_button.get_visible():
            self._exercise_fail("an unreadable scan offered Delete")
            return False
        missing_result = self.work / "no-result"
        missing_result.mkdir()
        self.out_dir = missing_result
        self._show_report(self._unreadable_scan_report())
        if self.delete_button.get_visible() or (missing_result / "result.json").is_file():
            self._exercise_fail("Delete was offered without a result file")
            return False
        self.out_dir = None
        self._exercise_report = None
        self._clear_results()
        self.summary.set_text("")
        self._set_line(self.context_note, "")
        self._set_line(self.result_note, "")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self._show_report(
            {
                "status": "complete",
                "summary": PHRASES["clearance"],
                "disclosure": ["Nothing is uploaded.", PHRASES["clearance"]],
                "candidates": [],
                "inventory": [],
            }
        )
        self._go("results")
        detail_labels = self._labels_under(self.detail)
        if (
            self.summary.get_text() != PHRASES["clearance"]
            or "warn" in self.summary.get_css_classes()
            or self.stack.get_visible_child_name() != "results"
            or PHRASES["clearance"] not in self.result_note.get_text()
            or PHRASES["clearance"] in detail_labels
        ):
            self._exercise_fail(
                f"clearance was repeated under the headline: {self.summary.get_text()!r} {detail_labels!r}"
            )
            return False
        if os.environ.get("OPENWORLD_CLEARANCE_SHOT"):
            context = GLib.MainContext.default()
            deadline = time.time() + 1.2
            while time.time() < deadline:
                context.iteration(False)
                time.sleep(0.05)
        self._grab(os.environ.get("OPENWORLD_CLEARANCE_SHOT"))
        self._exercise_report = None
        self._clear_results()
        self.summary.set_text("")
        self._set_line(self.context_note, "")
        self._set_line(self.result_note, "")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self.choose_another()
        missing_first = str(Path(path).with_name("no-such-photo.png"))
        self.choose_file(missing_first)
        root = self.window.get_child()
        first = root.get_first_child() if root is not None else None
        if (
            self.stack.get_visible_child_name() != "choose"
            or self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not self.pick_notice.get_visible()
            or "warn" not in self.pick_notice.get_css_classes()
            or first is not self.pick_notice
            or self.input_path is not None
        ):
            self._exercise_fail(
                f"a missing file left the first page: {self.pick_notice.get_text()!r} {self.input_path!r}"
            )
            return False
        self.choose_file(path)
        aged = time.time() - 40 * 24 * 3600
        os.utime(path, (aged, aged))
        self.choose_file(path)
        if (
            self.warn_label.get_text() != PHRASES["old"]
            or not self.warn_label.get_visible()
            or self.estimate_warn.get_text() != PHRASES["old"]
            or not self.estimate_warn.get_visible()
        ):
            self._exercise_fail(f"old file warning missing: {self.warn_label.get_text()!r} {self.estimate_warn.get_text()!r}")
            return False
        now = time.time()
        os.utime(path, (now, now))
        self.choose_file(path)
        if (
            self.warn_label.get_text()
            or self.warn_label.get_visible()
            or self.estimate_warn.get_text()
            or self.estimate_warn.get_visible()
        ):
            self._exercise_fail(f"fresh file warned: {self.warn_label.get_text()!r} {self.estimate_warn.get_text()!r}")
            return False
        long_path = self.work / (("A" * 180) + ".png")
        long_path.write_bytes(Path(path).read_bytes())
        self.choose_file(str(long_path))
        context = GLib.MainContext.default()
        for _ in range(30):
            context.iteration(False)
        layout = self.file_label.get_layout()
        line_count = layout.get_line_count() if layout is not None else 1
        minimum = self.file_label.get_preferred_size().minimum_size.width
        if (
            not self.file_label.get_wrap()
            or self.file_label.get_wrap_mode() != Pango.WrapMode.WORD_CHAR
            or minimum > 400
            or self.window.get_width() > 1200
            or line_count < 2
            or self.file_label.get_text() != long_path.name
        ):
            self._exercise_fail(
                f"a long file name widened the window: {self.window.get_width()} {minimum} {line_count} {self.file_label.get_wrap()}"
            )
            return False
        self.choose_file(path)
        missing = str(Path(path).with_name("no-such-photo.png"))
        self.choose_file(missing)
        folder = Path(path).with_name("not-a-file-dir")
        folder.mkdir(exist_ok=True)
        self.choose_file(str(folder))
        unreadable = Path(path).with_name("unreadable-photo.bin")
        unreadable.write_bytes(b"not readable")
        unreadable.chmod(0)
        try:
            if os.access(unreadable, os.R_OK):
                self._exercise_fail("this account can still read a mode-0 file")
                return False
            self.choose_file(str(unreadable))
        finally:
            unreadable.chmod(0o644)
            unreadable.unlink(missing_ok=True)
        root = self.window.get_child()
        first = root.get_first_child() if root is not None else None
        if (
            self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not self.pick_notice.get_visible()
            or first is not self.pick_notice
            or self.input_path != path
            or self.stack.get_visible_child_name() != "device"
        ):
            self._exercise_fail(
                f"an unreadable file changed the pick: {self.pick_notice.get_text()!r} {self.input_path!r}"
            )
            return False
        self.load_bundles()
        self._go("size")
        self.refresh_estimate()
        self._go("estimate")
        root = self.window.get_child()
        first = root.get_first_child() if root is not None else None
        if (
            self.stack.get_visible_child_name() != "estimate"
            or self.estimate_title.get_text() != "Estimate"
            or self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not self.pick_notice.get_visible()
            or "warn" not in self.pick_notice.get_css_classes()
            or first is not self.pick_notice
            or self.input_path != path
            or not self.primary.get_visible()
        ):
            self._exercise_fail(
                "continuing cleared the unread-file sentence: "
                f"{self.estimate_title.get_text()!r} {self.pick_notice.get_text()!r}"
            )
            return False
        self.choose_file(path)
        if self.pick_notice.get_visible() or self.input_path != path:
            self._exercise_fail("a readable file kept the refusal")
            return False
        self.measured_button.set_active(True)
        if self.coverage != "measured" or not self.brief_label.get_visible():
            self._exercise_fail("measured coverage did not show the brief-face warning")
            return False
        self.complete_button.set_active(True)
        if self.coverage != "complete" or self.brief_label.get_visible():
            self._exercise_fail("complete coverage still shows the brief-face warning")
            return False
        junk = Path(path).with_name("not-a-photo.txt")
        junk.write_text("not a photo\n")
        self.choose_file(str(junk))
        self.load_bundles()
        self._go("size")
        size_hint = self.size_hint.get_text()
        if (
            "A face under 64 px on that image is left out." not in size_hint
            or 'the label is "Not compared."' not in size_hint
        ):
            self._exercise_fail(f"the size page did not explain what is left out: {size_hint!r}")
            return False
        if (
            self.size_buttons["640"].get_label() != "640 px on the long side. Selected."
            or self.size_buttons["full"].get_label() != "Full resolution"
            or self.complete_button.get_label() != "Complete. Every decoded frame. Selected."
            or self.measured_button.get_label() != "Measured. 5 frames a second, plus the tracker."
        ):
            self._exercise_fail(
                "the size page did not mark the choice: "
                f"{self.size_buttons['640'].get_label()!r} {self.complete_button.get_label()!r}"
            )
            return False
        size_shot = os.environ.get("OPENWORLD_SIZE_SHOT")
        if size_shot:
            subprocess.run(
                ["xdotool", "search", "--name", "^OpenWorld$", "windowmove", "40", "40"],
                check=False,
            )
            context = GLib.MainContext.default()
            deadline = time.time() + 0.4
            while time.time() < deadline:
                context.iteration(False)
                time.sleep(0.05)
            self._grab(size_shot)
        self.refresh_estimate()
        self._go("estimate")
        if (
            self.primary.get_visible()
            or self.primary.get_sensitive()
            or self.estimate_title.get_text() != "Bad codec or unreadable file. Refusing."
            or self.estimate_body.get_visible()
            or self.estimate_body.get_text()
        ):
            self._exercise_fail(
                f"a bad file offered Analyze: {self.estimate_title.get_text()!r} {self.estimate_body.get_text()!r}"
            )
            return False
        if (
            self.missing_button.get_visible()
            or self.wanted_button.get_visible()
            or self.class_label.get_visible()
            or "Choose missing, wanted, or both." in self.estimate_body.get_text()
            or "Estimate" == self.estimate_title.get_text()
        ):
            self._exercise_fail("a refused estimate asked for a poster class")
            return False
        missing_on_refusal = str(Path(path).with_name("no-such-photo-on-refused-estimate.png"))
        self.choose_file(missing_on_refusal)
        root = self.window.get_child()
        first = root.get_first_child() if root is not None else None
        if (
            self.stack.get_visible_child_name() != "estimate"
            or self.estimate_title.get_text() != "Bad codec or unreadable file. Refusing."
            or "warn" not in self.estimate_title.get_css_classes()
            or self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not self.pick_notice.get_visible()
            or "warn" not in self.pick_notice.get_css_classes()
            or first is not self.pick_notice
            or self.input_path != str(junk)
            or self.primary.get_visible()
            or self.missing_button.get_visible()
            or self.wanted_button.get_visible()
        ):
            self._exercise_fail(
                "a refused estimate renamed itself when a file could not be read: "
                f"{self.estimate_title.get_text()!r} {self.pick_notice.get_text()!r}"
            )
            return False
        aged = time.time() - 40 * 24 * 3600
        os.utime(junk, (aged, aged))
        self.choose_file(str(junk))
        self.load_bundles()
        self._go("size")
        self.refresh_estimate()
        self._go("estimate")
        if (
            self.estimate_warn.get_visible()
            or self.warn_label.get_text() != PHRASES["old"]
            or self.estimate_title.get_text() != "Bad codec or unreadable file. Refusing."
        ):
            self._exercise_fail(
                f"a refused estimate kept the old-file warning: {self.estimate_title.get_text()!r} {self.estimate_warn.get_visible()}"
            )
            return False
        refusal_shot = os.environ.get("OPENWORLD_REFUSAL_SHOT")
        if refusal_shot:
            subprocess.run(
                ["xdotool", "search", "--name", "^OpenWorld$", "windowmove", "40", "40"],
                check=False,
            )
            context = GLib.MainContext.default()
            deadline = time.time() + 0.4
            while time.time() < deadline:
                context.iteration(False)
                time.sleep(0.05)
            self._grab(refusal_shot)
        self.go_back()
        if self.stack.get_visible_child_name() != "size" or not self.primary.get_visible() or not self.primary.get_sensitive():
            self._exercise_fail("Back left Continue disabled after a refusal")
            return False
        self.choose_file(path)
        if not self.primary.get_sensitive() or self.stack.get_visible_child_name() != "device":
            self._exercise_fail("the next file kept Continue disabled")
            return False
        saved_bundles = self.bundles
        empty = tempfile.mkdtemp(prefix="openworld-empty-bundles-")
        try:
            self.bundles = empty
            self.load_bundles()
            if (
                self.rows
                or self.bundle_title.get_text() != "The scan program is not on this device. Refusing."
                or self.bundle_hint.get_visible()
                or self.bundle_scroll.get_visible()
            ):
                self._exercise_fail(f"an empty catalog stayed silent: {self.rows!r} {self.bundle_title.get_text()!r}")
                return False
            self._show("bundle")
            bundle_shot = os.environ.get("OPENWORLD_BUNDLE_SHOT")
            if bundle_shot:
                subprocess.run(
                    ["xdotool", "search", "--name", "^OpenWorld$", "windowmove", "40", "40"],
                    check=False,
                )
                context = GLib.MainContext.default()
                deadline = time.time() + 0.4
                while time.time() < deadline:
                    context.iteration(False)
                    time.sleep(0.05)
                self._grab(bundle_shot)
            if self.primary.get_sensitive():
                self._exercise_fail("an empty catalog offered Continue")
                return False
            self.on_primary()
            if self.stack.get_visible_child_name() != "bundle":
                self._exercise_fail("an empty catalog left the bundle page")
                return False
            self.bundles = os.path.join(empty, "missing")
            self.load_bundles()
            if (
                self.rows
                or self.bundle_title.get_text() != "The bundle catalog could not be read. Refusing."
                or self.bundle_hint.get_visible()
            ):
                self._exercise_fail(
                    f"an unreadable catalog used the missing-program sentence: {self.rows!r} {self.bundle_title.get_text()!r}"
                )
                return False
            self._show("bundle")
            if self.primary.get_sensitive():
                self._exercise_fail("an unreadable catalog offered Continue")
                return False
            self.on_primary()
            if self.stack.get_visible_child_name() != "bundle":
                self._exercise_fail("an unreadable catalog left the bundle page")
                return False
            saved_bin = self.bin
            try:
                garbage = os.path.join(empty, "garbage.sh")
                with open(garbage, "w", encoding="utf-8") as handle:
                    handle.write("#!/bin/sh\nprintf '%s\\n' 'not json'\n")
                os.chmod(garbage, 0o755)
                self.bin = garbage
                self.load_bundles()
                if (
                    self.rows
                    or self.bundle_title.get_text() != "The bundle catalog could not be read. Refusing."
                    or self.bundle_hint.get_visible()
                ):
                    self._exercise_fail(
                        f"a program answer that is not a catalog used another sentence: {self.bundle_title.get_text()!r}"
                    )
                    return False
                self.refresh_estimate()
                if (
                    self.estimate_title.get_text() != "The scan could not be read. Refusing."
                    or self.estimate_body.get_visible()
                    or self.estimate_ok
                    or self.can_analyze
                ):
                    self._exercise_fail(
                        f"a program answer that is not an estimate stayed runnable: {self.estimate_title.get_text()!r}"
                    )
                    return False
                blank = os.path.join(empty, "blank.sh")
                with open(blank, "w", encoding="utf-8") as handle:
                    handle.write("#!/bin/sh\nexit 0\n")
                os.chmod(blank, 0o755)
                self.bin = blank
                self.load_bundles()
                if self.bundle_title.get_text() != "The bundle catalog could not be read. Refusing.":
                    self._exercise_fail(
                        f"an empty program answer used another sentence: {self.bundle_title.get_text()!r}"
                    )
                    return False
                self.bin = os.path.join(empty, "missing-bin")
                self.load_bundles()
                if self.bundle_title.get_text() != "The scan program is not on this device. Refusing.":
                    self._exercise_fail(
                        f"a missing program used another sentence: {self.bundle_title.get_text()!r}"
                    )
                    return False
            finally:
                self.bin = saved_bin
        finally:
            self.bundles = saved_bundles
            shutil.rmtree(empty, ignore_errors=True)
        self.load_bundles()
        if (
            not self.rows
            or self.bundle_title.get_text() != "Model bundle"
            or not self.bundle_hint.get_visible()
            or not self.bundle_scroll.get_visible()
        ):
            self._exercise_fail(
                f"the bundle page kept the missing-program sentence after the catalog returned: {self.bundle_title.get_text()!r}"
            )
            return False
        self.bundle_id = "not-in-the-catalog"
        self.load_bundles()
        if self.bundle_id != "fast":
            self._exercise_fail(f"a missing bundle stayed selected: {self.bundle_id}")
            return False
        self._go("bundle")
        if not self.primary.get_sensitive():
            self._exercise_fail("a catalog with a model left Continue off")
            return False
        accurate = self._bundle_row("accurate")
        if accurate is None:
            self._exercise_fail("Accurate is not in the catalog")
            return False
        self.bundle_list.select_row(accurate)
        if self.bundle_id != "accurate":
            self._exercise_fail(f"selecting Accurate left {self.bundle_id}")
            return False
        if accurate.bundle_name_label.get_text() != "Accurate. Selected." or self._bundle_row("fast").bundle_name_label.get_text() != "Fast":
            self._exercise_fail("the bundle page did not mark Accurate")
            return False
        self.go_back()
        if self.stack.get_visible_child_name() != "device":
            self._exercise_fail("Back from the bundle page did not return to the file page")
            return False
        self.load_bundles()
        if self.bundle_id != "accurate":
            self._exercise_fail(f"Continue replaced the chosen bundle with {self.bundle_id}")
            return False
        fast = self._bundle_row("fast")
        if fast is None:
            self._exercise_fail("Fast is not in the catalog")
            return False
        self.bundle_list.select_row(fast)
        if self.bundle_id != "fast":
            self._exercise_fail(f"selecting Fast left {self.bundle_id}")
            return False
        if fast.bundle_name_label.get_text() != "Fast. Selected." or self._bundle_row("accurate").bundle_name_label.get_text() != "Accurate":
            self._exercise_fail("the bundle page did not mark Fast")
            return False
        self._go("bundle")
        self.long_side = "640"
        self.coverage = "complete"
        self.refresh_estimate()
        self._go("estimate")
        text = self.estimate_body.get_text()
        if self.estimate_title.get_text() != "Estimate" or not self.estimate_body.get_visible():
            self._exercise_fail(f"a runnable estimate was titled {self.estimate_title.get_text()!r}")
            return False
        if "CPU" not in text or "second" not in text.lower() and "Less than" not in text:
            self._exercise_fail(f"estimate missing time or CPU note: {text}")
            return False
        if "Fast. 640 px on the long side. Every decoded frame." not in text or "Bundle fast" in text or "Coverage complete" in text:
            self._exercise_fail(f"estimate did not repeat the choice in plain words: {text}")
            return False
        self.coverage = "measured"
        self.refresh_estimate()
        measured = self.estimate_body.get_text()
        choice = "Fast. 640 px on the long side. 5 frames a second, plus the tracker."
        if (
            choice not in measured
            or PHRASES["brief"] not in measured
            or measured.find(choice) > measured.find(PHRASES["brief"])
        ):
            self._exercise_fail(f"the measured estimate did not warn after the choice: {measured!r}")
            return False
        self.coverage = "complete"
        self.refresh_estimate()
        if (
            not self.missing_button.get_visible()
            or not self.wanted_button.get_visible()
            or not self.class_label.get_visible()
            or self.class_label.get_text() != "Missing and wanted."
            or self._class_args()
            or not self.primary.get_visible()
        ):
            self._exercise_fail(f"classes did not start on: {self.class_label.get_text()!r} {self._class_args()}")
            return False
        self.wanted_button.set_active(False)
        if (
            self.class_label.get_text() != "Missing."
            or self.missing_button.get_label() != "Missing"
            or self.wanted_button.get_label() != "Wanted"
            or self._class_args() != ["--no-wanted"]
            or not self.primary.get_sensitive()
        ):
            self._exercise_fail(f"wanted off did not leave missing: {self.class_label.get_text()!r} {self.missing_button.get_label()!r} {self.wanted_button.get_label()!r} {self._class_args()}")
            return False
        self.missing_button.set_active(False)
        if self.primary.get_sensitive() or not self.primary.get_visible() or self.class_label.get_text() != "Choose missing, wanted, or both.":
            self._exercise_fail(f"both classes off still offered Analyze: {self.class_label.get_text()!r}")
            return False
        self.missing_button.set_active(True)
        self.wanted_button.set_active(True)
        if not self.primary.get_sensitive() or self.class_label.get_text() != "Missing and wanted." or self._class_args():
            self._exercise_fail("restoring both classes left Analyze off")
            return False
        self._show_report(
            {
                "status": "complete",
                "summary": PHRASES["clearance"],
                "class_note": "Missing and wanted.",
                "warnings": [PHRASES["old"], "The file timestamps disagree."],
                "disclosure": ["Nothing is uploaded."],
                "candidates": [
                    {
                        "wording": PHRASES["possible"],
                        "uncertainty": "",
                        "poster_title": "",
                        "poster_class_label": "Missing",
                        "fbi_url": "https://www.fbi.gov/wanted",
                    }
                ],
                "inventory": [],
            }
        )
        labels = self._labels_under(self.detail)
        if "Missing" not in labels or " (Missing)" in labels:
            self._exercise_fail(f"a poster with no title lost its class: {labels!r}")
            return False
        if (
            PHRASES["old"] not in self.warning_note.get_text()
            or "The file timestamps disagree." not in self.warning_note.get_text()
            or not self.warning_note.get_visible()
            or PHRASES["old"] in self.context_note.get_text()
            or "timestamps" in self.context_note.get_text()
            or "Missing and wanted." not in self.context_note.get_text()
        ):
            self._exercise_fail(
                f"a warning was mixed into the class line: {self.context_note.get_text()!r} {self.warning_note.get_text()!r}"
            )
            return False
        self._clear_results()
        self._show_report(
            {
                "status": "complete",
                "summary": PHRASES["possible"],
                "candidates": [
                    {
                        "wording": PHRASES["possible"],
                        "uncertainty": "Score 0.90.",
                        "poster_title": "Fixture subject A",
                        "poster_class_label": "Missing",
                        "fbi_url": "",
                    }
                ],
                "inventory": [],
                "disclosure": ["Nothing is uploaded."],
            }
        )
        labels = self._labels_under(self.detail)
        buttons = self._button_labels(self.detail)
        if (
            PHRASES["possible"] not in labels
            or "Fixture subject A (Missing)" not in labels
            or "Open FBI page" in buttons
        ):
            self._exercise_fail(f"a candidate with no page lost its card: {labels!r} {buttons!r}")
            return False
        blocker = self.work / "posters"
        blocker.write_bytes(b"keep")
        self.start_scan()
        if (
            self.summary.get_text() != "The poster pack could not be read. Refusing."
            or "warn" not in self.summary.get_css_classes()
            or blocker.read_bytes() != b"keep"
            or self.delete_button.get_visible()
            or self.primary.get_label() != "Choose another file"
            or not self.primary.get_sensitive()
            or "Nothing is uploaded." not in self.result_note.get_text()
            or "On-device does not mean the file is real." not in self.result_note.get_text()
            or PHRASES["clearance"] in self.result_note.get_text()
            or self.stack.get_visible_child_name() != "results"
        ):
            self._exercise_fail(
                f"a poster pack file still started a scan: {self.summary.get_text()!r} {self.result_note.get_text()!r}"
            )
            return False
        missing_on_pack = str(Path(path).with_name("no-such-photo-on-poster-pack.png"))
        self.choose_file(missing_on_pack)
        root = self.window.get_child()
        first = root.get_first_child() if root is not None else None
        if (
            self.stack.get_visible_child_name() != "results"
            or self.summary.get_text() != "The poster pack could not be read. Refusing."
            or "warn" not in self.summary.get_css_classes()
            or self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not self.pick_notice.get_visible()
            or "warn" not in self.pick_notice.get_css_classes()
            or first is not self.pick_notice
            or "Nothing is uploaded." not in self.result_note.get_text()
            or PHRASES["clearance"] in self.result_note.get_text()
            or self.delete_button.get_visible()
        ):
            self._exercise_fail(
                "a poster pack refusal renamed itself when a file could not be read: "
                f"{self.summary.get_text()!r} {self.pick_notice.get_text()!r}"
            )
            return False
        result_shot = os.environ.get("OPENWORLD_RESULT_SHOT")
        if result_shot:
            subprocess.run(
                ["xdotool", "search", "--name", "^OpenWorld$", "windowmove", "40", "40"],
                check=False,
            )
            context = GLib.MainContext.default()
            deadline = time.time() + 0.4
            while time.time() < deadline:
                context.iteration(False)
                time.sleep(0.05)
            self._grab(result_shot)
        blocker.unlink()
        self._exercise_report = None
        self._set_line(self.context_note, "Missing and wanted.")
        self._set_line(self.result_note, PHRASES["clearance"])
        self.reason.set_text("The file was not fully decoded.")
        self.reason.set_visible(True)
        self.warning_note.set_text(PHRASES["old"])
        self.warning_note.set_visible(True)
        self.start_scan()
        if (
            self.summary.get_text() != "Scanning"
            or self.context_note.get_text()
            or self.context_note.get_visible()
            or self.result_note.get_text()
            or self.result_note.get_visible()
            or self.reason.get_text()
            or self.reason.get_visible()
            or self.warning_note.get_text()
            or self.warning_note.get_visible()
            or self.primary.get_label() != "Scanning"
            or self.delete_button.get_visible()
            or self.primary.get_sensitive()
            or self.back.get_sensitive()
            or self.stack.get_visible_child_name() != "results"
            or not self.pick_notice.get_visible()
            or self.pick_notice.get_text() != "The file could not be read. Refusing."
        ):
            self._exercise_fail(
                f"Scanning kept the previous result: {self.summary.get_text()!r} {self.result_note.get_text()!r} {self.reason.get_text()!r}"
            )
            return False
        saved_out = self.out_dir
        probe = self.work / "probe-crops"
        (probe / "crops").mkdir(parents=True)
        shutil.copy(path, probe / "crops" / "plate.png")
        self.out_dir = probe
        self._add_crop({"kind": "plate", "crop": "crops/plate.png", "label": "No poster publishes a plate.", "frame_label": "Frame 1."})
        self._add_crop({"kind": "vehicle", "crop": "crops/plate.png", "label": "A vehicle is not a person."})
        self._add_crop({"kind": "audio", "crop": "crops/plate.png", "label": "skip me"})
        self._add_crop({"kind": "face", "label": "Not compared."})
        live = self._strip_labels()
        self.out_dir = saved_out
        if (
            "No poster publishes a plate." not in live
            or "Frame 1." not in live
            or "A vehicle is not a person." not in live
            or "skip me" in live
            or "Not compared." in live
            or not self.strip_heading.get_visible()
        ):
            self._exercise_fail(f"a plate or a vehicle stayed off the running scan: {live}")
            return False
        self._clear_strip()
        self.strip_heading.set_visible(False)
        GLib.timeout_add(200, self._exercise_wait, 0)
        return False

    def _exercise_wait(self, ticks: int) -> bool:
        report = getattr(self, "_exercise_report", None)
        if report is None:
            if ticks > 100:
                self._exercise_fail("scan timed out")
                return False
            GLib.timeout_add(200, self._exercise_wait, ticks + 1)
            return False
        blob = json.dumps(report)
        for phrase in (PHRASES["possible"], PHRASES["not_compared"], PHRASES["clearance"], PHRASES["leaving"]):
            if phrase not in blob and phrase not in self.result_note.get_text() and phrase not in self.summary.get_text():
                # leaving is on the candidate, which is inside the report
                if phrase not in blob:
                    self._exercise_fail(f"missing {phrase}")
                    return False
        if "warn" in self.summary.get_css_classes():
            self._exercise_fail("a finished scan kept the refusal color")
            return False
        if not self.context_note.get_visible() or not self.result_note.get_visible():
            self._exercise_fail("the result hid the lines under the headline")
            return False
        if "Missing and wanted." not in self.context_note.get_text():
            self._exercise_fail(f"the result did not name the classes under the headline: {self.context_note.get_text()!r}")
            return False
        if "1 frame analyzed." not in self.context_note.get_text():
            self._exercise_fail(f"the result did not say how many frames were analyzed: {self.context_note.get_text()!r}")
            return False
        if "640 px on the long side." not in self.context_note.get_text():
            self._exercise_fail(f"the result did not repeat the detection size: {self.context_note.get_text()!r}")
            return False
        if "Every decoded frame." not in self.context_note.get_text():
            self._exercise_fail(f"the result did not repeat the coverage: {self.context_note.get_text()!r}")
            return False
        result_shot = os.environ.get("OPENWORLD_RESULT_SHOT")
        if result_shot and not getattr(self, "_result_shot_saved", False):
            self._result_shot_saved = True
            subprocess.run(
                ["xdotool", "search", "--name", "^OpenWorld$", "windowmove", "40", "40"],
                check=False,
            )
            adjustment = self.results_scroll.get_vadjustment()
            adjustment.set_value(0)
            self._grab(result_shot)
        if report.get("status") != "complete":
            self._exercise_fail(report.get("message", "not complete"))
            return False
        if self.pick_notice.get_visible():
            self._exercise_fail("a finished scan kept the unread-file sentence")
            return False
        labels = self._strip_labels()
        for phrase in (PHRASES["possible"], PHRASES["not_compared"], "A vehicle is not a person."):
            if phrase not in labels:
                self._exercise_fail(f"strip missing {phrase}: {labels}")
                return False
        crops = [label for label in labels if not label.startswith("Frame ")]
        if not crops or labels.count("Frame 1.") != len(crops):
            self._exercise_fail(f"a crop did not name its frame: {labels}")
            return False
        if self.scan_thread is not None or not self.back.get_sensitive():
            self._exercise_fail("results did not return control of the window")
            return False
        if self.primary.get_label() != "Choose another file":
            self._exercise_fail(f"results button is {self.primary.get_label()!r}")
            return False
        fbi_buttons = [label for label in self._button_labels(self.detail) if label == "Open FBI page"]
        expected = len(report.get("candidates") or [])
        if len(fbi_buttons) != expected or expected < 1:
            self._exercise_fail(f"expected {expected} FBI buttons, saw {fbi_buttons}")
            return False
        card_text = " ".join(self._labels_under(self.detail))
        if card_text.count("Crop") < expected or card_text.count("Frame 1.") < expected:
            self._exercise_fail(f"a candidate card is missing its crop or frame: {card_text}")
            return False
        if "Score " not in card_text or "Cosine" in card_text or "FIX123" not in card_text:
            self._exercise_fail(f"the card did not explain the score and the plate: {card_text}")
            return False
        for candidate in report.get("candidates") or []:
            uncertainty = candidate.get("uncertainty") or ""
            if PHRASES["possible"] in uncertainty:
                self._exercise_fail(f"the score line repeated the candidate sentence: {uncertainty}")
                return False
        if "(missing)" in card_text or "(wanted)" in card_text or "(Missing)" not in card_text or "(Wanted)" not in card_text:
            self._exercise_fail(f"a card showed the raw poster class: {card_text}")
            return False
        if self.strip_heading.get_text() != "Crops from this file." or not self.strip_heading.get_visible():
            self._exercise_fail("the other crops are not labeled under the cards")
            return False
        result = self.out_dir
        if result is None or not (result / "result.json").is_file() or not self.delete_button.get_visible():
            self._exercise_fail("no result to delete")
            return False
        headline = self.summary.get_text()
        scene = self.input_path
        self.go_back()
        os.chmod(result, 0o555)
        self.start_scan()
        os.chmod(result, 0o755)
        root = self.window.get_child()
        first = root.get_first_child() if root is not None else None
        if (
            self.stack.get_visible_child_name() != "estimate"
            or self.summary.get_text() != headline
            or self.pick_notice.get_text() != "The result could not be deleted."
            or not self.pick_notice.get_visible()
            or "warn" not in self.pick_notice.get_css_classes()
            or first is not self.pick_notice
            or self.estimate_delete.get_visible()
            or self._labels_under(self.page_estimate).count("The result could not be deleted.") != 0
            or self.primary.get_label() != "Analyze"
            or self.scan_thread is not None
            or not (result / "result.json").is_file()
        ):
            self._exercise_fail(
                "Analyze left the estimate while the result could not be deleted: "
                f"{self.stack.get_visible_child_name()!r} {self.summary.get_text()!r} {self.pick_notice.get_text()!r}"
            )
            return False
        self.pick_notice.set_text("The file could not be read. Refusing.")
        self.pick_notice.set_visible(True)
        os.chmod(result, 0o555)
        self.start_scan()
        os.chmod(result, 0o755)
        if (
            self.stack.get_visible_child_name() != "estimate"
            or self.summary.get_text() != headline
            or self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not self.pick_notice.get_visible()
            or self.estimate_delete.get_text() != "The result could not be deleted."
            or not self.estimate_delete.get_visible()
            or "warn" not in self.estimate_delete.get_css_classes()
            or self._labels_under(self.page_estimate).count("The result could not be deleted.") != 1
            or self.primary.get_label() != "Analyze"
            or self.scan_thread is not None
            or not (result / "result.json").is_file()
        ):
            self._exercise_fail(
                "Analyze replaced the unread-file sentence when the result could not be deleted: "
                f"{self.pick_notice.get_text()!r} {self.estimate_delete.get_text()!r}"
            )
            return False
        while self.stack.get_visible_child_name() != "device" and len(self.history) > 1:
            self.go_back()
        os.chmod(result, 0o555)
        other = str(Path(scene or "").with_name("second-photo.png"))
        shutil.copy(scene or "", other)
        self.choose_file(str(Path(scene or "").with_name("no-such-photo-before-delete.png")))
        self.choose_file(other)
        os.chmod(result, 0o755)
        root = self.window.get_child()
        first = root.get_first_child() if root is not None else None
        if (
            self.stack.get_visible_child_name() != "device"
            or self.input_path != scene
            or self.pick_notice.get_text() != "The result could not be deleted."
            or not self.pick_notice.get_visible()
            or "warn" not in self.pick_notice.get_css_classes()
            or first is not self.pick_notice
            or not (result / "result.json").is_file()
        ):
            self._exercise_fail(
                "a file chosen from the device page hid a result that could not be deleted: "
                f"{self.pick_notice.get_text()!r} {self.input_path!r}"
            )
            return False
        self.on_primary()
        self.on_primary()
        self.on_primary()
        if (
            self.stack.get_visible_child_name() != "estimate"
            or self.pick_notice.get_text() != "The result could not be deleted."
            or not self.pick_notice.get_visible()
            or self._labels_under(self.page_estimate).count("The result could not be deleted.") != 0
        ):
            self._exercise_fail(
                "the estimate repeated a result that could not be deleted: "
                f"{self.pick_notice.get_text()!r} {self.stack.get_visible_child_name()!r}"
            )
            return False
        self.go_back()
        if (
            self.stack.get_visible_child_name() != "size"
            or self.pick_notice.get_visible()
            or self.delete_notice.get_visible()
            or self.delete_notice.get_text()
        ):
            self._exercise_fail(
                "Back left the delete sentence on the size page: "
                f"{self.pick_notice.get_text()!r} {self.delete_notice.get_text()!r}"
            )
            return False
        self.pick_notice.set_visible(False)
        self.delete_notice.set_visible(False)
        self._go("results")
        missing_on_result = str(Path(scene or "").with_name("no-such-photo-on-results.png"))
        self.choose_file(missing_on_result)
        root = self.window.get_child()
        first = root.get_first_child() if root is not None else None
        if (
            self.stack.get_visible_child_name() != "results"
            or self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not self.pick_notice.get_visible()
            or first is not self.pick_notice
            or self.input_path != scene
            or self.summary.get_text() != headline
            or not self.delete_button.get_visible()
            or not (result / "result.json").is_file()
        ):
            self._exercise_fail(
                f"an unreadable file left the result: {self.pick_notice.get_text()!r} {self.summary.get_text()!r}"
            )
            return False
        saved = (result / "result.json").read_bytes()
        (result / "result.json").unlink()
        if (
            self.delete_result()
            or self.summary.get_text() != headline
            or "not an OpenWorld result" not in self.delete_notice.get_text()
            or not self.delete_notice.get_visible()
            or "warn" not in self.delete_notice.get_css_classes()
            or not result.exists()
            or not self.delete_button.get_visible()
            or PHRASES["possible"] not in self._labels_under(self.detail)
        ):
            self._exercise_fail(f"a refused delete replaced the result: {self.summary.get_text()!r} {self.delete_notice.get_text()!r}")
            return False
        (result / "result.json").write_bytes(saved)
        if (
            not self.delete_result()
            or result.exists()
            or self.summary.get_text() != "Deleted."
            or self.context_note.get_text()
            or self.context_note.get_visible()
            or self.result_note.get_text()
            or self.result_note.get_visible()
            or self.delete_button.get_visible()
            or self.delete_notice.get_visible()
        ):
            self._exercise_fail("result remained after delete")
            return False
        missing_on_deleted = str(Path(scene or "").with_name("no-such-photo-on-deleted.png"))
        self.choose_file(missing_on_deleted)
        root = self.window.get_child()
        first = root.get_first_child() if root is not None else None
        if (
            self.stack.get_visible_child_name() != "results"
            or self.summary.get_text() != "Deleted."
            or "warn" in self.summary.get_css_classes()
            or self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not self.pick_notice.get_visible()
            or "warn" not in self.pick_notice.get_css_classes()
            or first is not self.pick_notice
            or self.result_note.get_text()
            or self.delete_button.get_visible()
            or self.input_path != scene
        ):
            self._exercise_fail(
                "a deleted result renamed itself when a file could not be read: "
                f"{self.summary.get_text()!r} {self.pick_notice.get_text()!r} {self.result_note.get_text()!r}"
            )
            return False
        kept = self.work / "kept-result"
        kept.mkdir()
        (kept / "result.json").write_text("{}\n")
        self.out_dir = kept
        self.summary.set_text(headline)
        self._show_delete(True)
        self._show("results")
        held = threading.Event()
        self.scan_thread = threading.Thread(target=held.wait)
        self.scan_thread.start()
        kept_path = self.input_path
        self.choose_file(scene or "")
        if (
            self.stack.get_visible_child_name() != "results"
            or self.input_path != kept_path
            or not kept.exists()
            or self.out_dir != kept
        ):
            held.set()
            self.scan_thread.join(timeout=2)
            self.scan_thread = None
            self._exercise_fail("a file chosen during a scan replaced the scan")
            return False
        held.set()
        self.scan_thread.join(timeout=2)
        self.scan_thread = None
        self.choose_file(scene or "")
        if (
            self.stack.get_visible_child_name() != "device"
            or self.input_path != scene
            or kept.exists()
            or self.out_dir is not None
            or self.pick_notice.get_visible()
            or self.delete_button.get_visible()
        ):
            self._exercise_fail(
                f"a new file left the result with no Delete button: {self.stack.get_visible_child_name()!r} {kept.exists()}"
            )
            return False
        blocked, opened = self.leave_decision("https://www.fbi.gov.evil.com/wanted")
        if opened is not None or "FBI page" not in blocked:
            self._exercise_fail(f"lookalike host was allowed: {blocked} {opened}")
            return False
        page = (report.get("candidates") or [{}])[0].get("fbi_url") or ""
        message, allowed = self.leave_decision(page)
        if message != PHRASES["leaving"] or allowed != page:
            self._exercise_fail(f"FBI page was blocked: {message} {allowed}")
            return False
        self.choose_another()
        if (
            self.stack.get_visible_child_name() != "choose"
            or self.input_path is not None
            or self.choose_hint.get_text() != "Import a file you already have. Drop it here, or use Choose File. There is no camera."
        ):
            self._exercise_fail("choose another did not return to the start")
            return False
        orphan = self.work / "not-a-result"
        orphan.mkdir()
        (orphan / "notes.txt").write_text("keep")
        self.out_dir = orphan
        self.choose_another()
        if (
            self.stack.get_visible_child_name() != "choose"
            or self.input_path is not None
            or not (orphan / "notes.txt").is_file()
        ):
            self._exercise_fail("choose another deleted a folder that is not a result, or stayed put")
            return False
        stuck = self.work / "stuck-result"
        stuck.mkdir()
        (stuck / "result.json").write_text("{}\n")
        self.out_dir = stuck
        self._show("estimate")
        self.pick_notice.set_text("The file could not be read. Refusing.")
        self.pick_notice.set_visible(True)
        os.chmod(stuck, 0o555)
        self.start_scan()
        os.chmod(stuck, 0o755)
        if (
            self.stack.get_visible_child_name() != "estimate"
            or self.estimate_delete.get_text() != "The result could not be deleted."
            or not self.estimate_delete.get_visible()
            or self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not stuck.is_dir()
        ):
            self._exercise_fail(
                "Analyze dropped the delete sentence beside an unread file: "
                f"{self.estimate_delete.get_text()!r} {self.pick_notice.get_text()!r}"
            )
            return False
        self.choose_file(scene or "")
        self.on_primary()
        self.on_primary()
        self.on_primary()
        if (
            self.stack.get_visible_child_name() != "estimate"
            or self.estimate_delete.get_visible()
            or self.estimate_delete.get_text()
            or self._labels_under(self.page_estimate).count("The result could not be deleted.") != 0
            or self.pick_notice.get_visible()
            or stuck.exists()
            or self.input_path != scene
        ):
            self._exercise_fail(
                "a readable file left the delete sentence on the next estimate: "
                f"{self.estimate_delete.get_text()!r} {self.pick_notice.get_text()!r} {self.input_path!r}"
            )
            return False
        kept_aside = self.work / "result"
        kept_aside.mkdir()
        (kept_aside / "notes.txt").write_text("keep")
        os.chmod(kept_aside, 0o555)
        self.out_dir = kept_aside
        self.start_scan()
        if (
            self.out_dir == kept_aside
            or self.summary.get_text() != "Scanning"
            or self.stack.get_visible_child_name() != "results"
            or not (kept_aside / "notes.txt").is_file()
            or self.scan_thread is None
        ):
            os.chmod(kept_aside, 0o755)
            self._exercise_fail(
                "Analyze replaced a folder that is not a result: "
                f"{self.summary.get_text()!r} {self.out_dir}"
            )
            return False
        os.chmod(kept_aside, 0o755)
        if not self._exercise_large_report():
            return False
        Path(os.environ.get("OPENWORLD_STATUS", "/tmp/openworld-exercise.json")).write_text(
            json.dumps({"ok": True, "summary": report.get("summary"), "work": str(self.work)})
        )
        try:
            self._save_shot()
        finally:
            self.quit()
        return False

    def _exercise_large_report(self) -> bool:
        child = self.work / "wide-report.py"
        child.write_text(
            "import sys\n"
            "sys.stderr.write('{\"kind\":\"face\",\"label\":\"Possible candidate. Not an identification.\",\"crop\":\"crops/a.png\",\"frame_label\":\"Frame 1.\"}\\n')\n"
            "sys.stderr.flush()\n"
            "sys.stdout.write('{\"pad\":\"' + ('x' * 200000) + '\"}')\n"
            "sys.stdout.flush()\n"
        )
        proc = subprocess.Popen(
            ["python3", str(child)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        box: dict = {}

        def run() -> None:
            lines: list[str] = []
            box["stdout"] = drain_scan_pipes(proc, lines.append)
            box["lines"] = lines

        worker = threading.Thread(target=run, daemon=True)
        worker.start()
        worker.join(5)
        if worker.is_alive():
            proc.kill()
            worker.join(2)
            self._exercise_fail("a large report left the screen on Scanning")
            return False
        stdout = box.get("stdout") or ""
        lines = box.get("lines") or []
        if "Frame 1." not in "".join(lines) or len(stdout) < 200000 or not stdout.startswith("{\"pad\":"):
            self._exercise_fail(f"a large report dropped a crop or the report: {len(stdout)} {lines!r}")
            return False
        return True

    def _exercise_fail(self, message: str) -> None:
        Path(os.environ.get("OPENWORLD_STATUS", "/tmp/openworld-exercise.json")).write_text(
            json.dumps({"ok": False, "message": message})
        )
        self.quit()

    def _save_shot(self) -> None:
        self._grab(os.environ.get("OPENWORLD_SHOT"))

    def _grab(self, path: str | None) -> None:
        if not path:
            return
        context = GLib.MainContext.default()
        for _ in range(30):
            context.iteration(False)
        display = os.environ.get("DISPLAY", ":0")
        subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-f", "x11grab", "-video_size", "1200x800", "-i", display, "-frames:v", "1", path],
            check=False,
        )


def main() -> None:
    app = OpenWorld()
    raise SystemExit(app.run(["openworld_gtk.py"]))


if __name__ == "__main__":
    main()
