#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Desktop window for OpenWorld. The scan stays in the Rust library."""

from __future__ import annotations

import atexit
import json
import os
import shutil
import stat
import subprocess
import tempfile
import threading
import time
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, Gio, GLib, Gtk


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

    def do_activate(self) -> None:
        self.window = Gtk.ApplicationWindow(application=self, title="OpenWorld")
        self.window.set_default_size(980, 860)
        self._css()
        header = Gtk.HeaderBar()
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
            self.stack.add_named(page, name)
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
        self.file_label = Gtk.Label(xalign=0)
        self.file_label.add_css_class("section")
        self.warn_label = Gtk.Label(xalign=0, wrap=True)
        self.warn_label.add_css_class("warn")
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
        title = Gtk.Label(label="Model bundle", xalign=0)
        title.add_css_class("title")
        hint = Gtk.Label(
            label="Scores are not comparable across bundles. Results name the bundle you pick.",
            xalign=0,
            wrap=True,
        )
        hint.add_css_class("dim")
        self.bundle_list = Gtk.ListBox()
        self.bundle_list.set_selection_mode(Gtk.SelectionMode.SINGLE)
        self.bundle_list.connect("row-selected", self._on_bundle_row)
        scroll = Gtk.ScrolledWindow()
        scroll.set_child(self.bundle_list)
        scroll.set_vexpand(True)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        box.append(title)
        box.append(hint)
        box.append(scroll)
        return box

    def _size_page(self) -> Gtk.Widget:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Detection size", xalign=0)
        title.add_css_class("title")
        hint = Gtk.Label(
            label="Smaller frames are a resize of each decoded frame in memory. Evidence crops come from the original frame. Full resolution is slower.",
            xalign=0,
            wrap=True,
        )
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
            if group is None:
                group = button
            else:
                button.set_group(group)
            if value == "640":
                button.set_active(True)
            button.connect("toggled", self._on_size, value)
            self.size_buttons[value] = button
            box.append(button)
        cover = Gtk.Label(label="Coverage", xalign=0)
        cover.add_css_class("section")
        cover.set_margin_top(12)
        box.append(cover)
        self.complete_button = Gtk.CheckButton(label="Complete. Every decoded frame.")
        self.measured_button = Gtk.CheckButton(label="Measured. 5 frames a second, plus the tracker.")
        self.measured_button.set_group(self.complete_button)
        self.complete_button.set_active(True)
        self.complete_button.connect("toggled", self._on_coverage, "complete")
        self.measured_button.connect("toggled", self._on_coverage, "measured")
        self.brief_label = Gtk.Label(label=PHRASES["brief"], xalign=0)
        self.brief_label.set_visible(False)
        box.append(self.complete_button)
        box.append(self.measured_button)
        box.append(self.brief_label)
        return box

    def _estimate_page(self) -> Gtk.Widget:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        title = Gtk.Label(label="Estimate", xalign=0)
        title.add_css_class("title")
        self.estimate_body = Gtk.Label(xalign=0, wrap=True)
        self.missing_button = Gtk.CheckButton(label="Missing")
        self.wanted_button = Gtk.CheckButton(label="Wanted")
        self.missing_button.set_active(True)
        self.wanted_button.set_active(True)
        self.missing_button.connect("toggled", self._on_class)
        self.wanted_button.connect("toggled", self._on_class)
        self.class_label = Gtk.Label(label="Missing and wanted.", xalign=0, wrap=True)
        box.append(title)
        box.append(self.estimate_body)
        box.append(self.missing_button)
        box.append(self.wanted_button)
        box.append(self.class_label)
        return box

    def _results_page(self) -> Gtk.Widget:
        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.summary = Gtk.Label(xalign=0, wrap=True)
        self.summary.add_css_class("title")
        self.reason = Gtk.Label(xalign=0, wrap=True)
        self.reason.add_css_class("warn")
        self.reason.set_visible(False)
        self.context_note = Gtk.Label(xalign=0, wrap=True)
        self.result_note = Gtk.Label(xalign=0, wrap=True)
        self.result_note.add_css_class("dim")
        self.strip_heading = Gtk.Label(label="Crops from this file.", xalign=0)
        self.strip_heading.add_css_class("section")
        self.strip_heading.set_visible(False)
        self.strip = Gtk.FlowBox()
        self.strip.set_max_children_per_line(6)
        self.strip.set_selection_mode(Gtk.SelectionMode.NONE)
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
        inner.append(self.detail)
        inner.append(self.strip_heading)
        inner.append(self.strip)
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

    def pick_file(self) -> None:
        dialog = Gtk.FileDialog()
        dialog.open(self.window, None, self._file_chosen)

    def _file_chosen(self, dialog: Gtk.FileDialog, result: Gio.AsyncResult) -> None:
        try:
            chosen = dialog.open_finish(result)
        except GLib.Error:
            return
        if chosen is None:
            return
        path = chosen.get_path()
        if path:
            self.choose_file(path)
        else:
            self._refuse_pick()

    def _on_drop(self, _target, value, _x, _y) -> bool:
        path = value.get_path() if value is not None else None
        if path:
            self.choose_file(path)
        else:
            self._refuse_pick()
        return True

    def _refuse_pick(self) -> None:
        self.pick_notice.set_text("The file could not be read. Refusing.")
        self.pick_notice.set_visible(True)

    def choose_file(self, path: str) -> None:
        try:
            info = os.stat(path)
        except OSError:
            self._refuse_pick()
            return
        if not stat.S_ISREG(info.st_mode):
            self._refuse_pick()
            return
        self.pick_notice.set_visible(False)
        self.input_path = path
        self.file_label.set_text(Path(path).name)
        age = time.time() - os.stat(path).st_mtime
        if age > 30 * 24 * 3600:
            self.warn_label.set_text(PHRASES["old"])
        else:
            self.warn_label.set_text("")
        self.can_analyze = True
        self.history = ["choose"]
        self._go("device")

    def load_bundles(self) -> None:
        payload = self._run_json(["--json", "--bundles", self.bundles, "bundles"])
        self.rows = payload.get("bundles", [])
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

    def _on_size(self, button: Gtk.CheckButton, value: str) -> None:
        if button.get_active():
            self.long_side = value

    def _on_coverage(self, button: Gtk.CheckButton, value: str) -> None:
        if button.get_active():
            self.coverage = value
            self.brief_label.set_visible(value == "measured")

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
            self.estimate_body.set_text(payload.get("message", "Refusing."))
            self.estimate_ok = False
            self.can_analyze = False
            self._show_classes(False)
            return
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
        if self.coverage == "measured":
            lines.append(PHRASES["brief"])
        lines.append(self.choice_line())
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
        if self.scan_thread is not None and self.scan_thread.is_alive():
            return
        self._clear_results()
        self.context_note.set_text("")
        self.result_note.set_text("")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self.summary.set_text("Scanning")
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
        self.out_dir = self.work / "result"
        subprocess.check_call(
            [self.bin, "--bundles", self.bundles, "posters", "write-fixture", "--out", str(self.posters)],
            stdout=subprocess.DEVNULL,
        )
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
        )
        assert proc.stderr is not None
        for line in proc.stderr:
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            GLib.idle_add(self._add_crop, event)
        stdout = proc.stdout.read() if proc.stdout else ""
        proc.wait()
        try:
            report = json.loads(stdout)
        except json.JSONDecodeError:
            report = {"status": "refused", "summary": "The scan could not be read.", "disclosure": []}
        GLib.idle_add(self._show_report, report)

    def _add_crop(self, event: dict) -> bool:
        kind = event.get("kind")
        label = event.get("label") or ""
        rel = event.get("crop")
        if kind not in ("face", "plate", "vehicle") or not rel or not label or self.out_dir is None:
            return False
        path = self.out_dir / rel
        if not path.is_file():
            return False
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        picture = Gtk.Picture.new_for_filename(str(path))
        picture.set_size_request(112, 112)
        picture.set_can_shrink(False)
        caption = Gtk.Label(label=label, wrap=True, justify=Gtk.Justification.CENTER)
        caption.set_max_width_chars(18)
        box.append(picture)
        box.append(caption)
        self.strip.append(box)
        self.strip_heading.set_visible(True)
        return False

    def _show_report(self, report: dict) -> bool:
        self.scan_thread = None
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
        if report.get("class_note"):
            context.append(report["class_note"])
        if report.get("bundle_name"):
            context.append(f"Bundle: {report['bundle_name']}")
        if report.get("perception_note"):
            context.append(report["perception_note"])
        for warning in report.get("warnings") or []:
            context.append(warning)
        self.context_note.set_text("\n".join(context))
        self.result_note.set_text("\n".join(report.get("disclosure") or []))
        self.results_scroll.get_vadjustment().set_value(0)
        self._fill_strip(report)
        for candidate in report.get("candidates") or []:
            self.detail.append(self._candidate_card(candidate))
        self.primary.set_sensitive(True)
        self.primary.set_label("Choose another file")
        self.delete_notice.set_visible(False)
        self._show_delete(self.out_dir is not None)
        self.back.set_sensitive(len(self.history) > 1)
        self._exercise_report = report
        return False

    def _fill_strip(self, report: dict) -> None:
        self.strip.remove_all()
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
            picture = Gtk.Picture.new_for_filename(str(path))
            picture.set_size_request(112, 112)
            picture.set_can_shrink(False)
            caption = Gtk.Label(label=item.get("label", ""), wrap=True, justify=Gtk.Justification.CENTER)
            caption.set_max_width_chars(18)
            box.append(picture)
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
            inner = child.get_child()
            widget = inner.get_first_child() if inner is not None else None
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
        for key, caption in (("crop", "Crop"), ("frame", "Frame")):
            rel = candidate.get(key)
            if rel and self.out_dir is not None:
                path = self.out_dir / rel
                if path.is_file():
                    pair = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
                    picture = Gtk.Picture.new_for_filename(str(path))
                    picture.set_size_request(160, 120)
                    label = Gtk.Label(label=caption, xalign=0)
                    pair.append(picture)
                    pair.append(label)
                    images.append(pair)
        box.append(images)
        uncertainty = Gtk.Label(label=candidate.get("uncertainty", ""), xalign=0, wrap=True)
        poster = Gtk.Label(
            label=f"{candidate.get('poster_title', '')} ({candidate.get('poster_class', '')})",
            xalign=0,
            wrap=True,
        )
        box.append(uncertainty)
        box.append(poster)
        button = Gtk.Button(label="Open FBI page")
        button.set_halign(Gtk.Align.START)
        button.connect("clicked", lambda *_c, url=candidate.get("fbi_url", ""): self.confirm_leave(url))
        box.append(button)
        frame.set_child(box)
        return frame

    def leave_decision(self, url: str) -> tuple[str, str | None]:
        payload = self._run_json(["--json", "leave", "--url", url])
        allowed = payload.get("url") or ""
        message = payload.get("message") or ""
        if message != PHRASES["leaving"] or not allowed:
            return message or "OpenWorld only opens an FBI page.", None
        return message, allowed

    def confirm_leave(self, url: str) -> None:
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
        dialog.format_secondary_text(allowed)
        dialog.connect("response", self._leave_response, allowed)
        dialog.present()

    def _leave_response(self, dialog: Gtk.MessageDialog, response: int, url: str) -> None:
        dialog.destroy()
        if response == Gtk.ResponseType.ACCEPT and url:
            Gio.AppInfo.launch_default_for_uri(url, None)

    def delete_result(self) -> bool:
        if self.out_dir is None:
            return True
        if not self.out_dir.exists():
            self._mark_deleted()
            return True
        payload = self._run_json(["--json", "delete", "--out", str(self.out_dir)])
        if not payload.get("deleted"):
            self.delete_notice.set_text(payload.get("message") or "The result could not be deleted.")
            self.delete_notice.set_visible(True)
            return False
        self._mark_deleted()
        return True

    def _show_delete(self, visible: bool) -> None:
        self.delete_button.set_visible(visible)
        self.delete_button.set_sensitive(visible)

    def _mark_deleted(self) -> None:
        self._clear_results()
        self.context_note.set_text("")
        self.result_note.set_text("")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self.summary.set_text("Deleted.")
        self.out_dir = None
        self.delete_notice.set_text("")
        self.delete_notice.set_visible(False)
        self._show_delete(False)

    def _remove_work(self) -> None:
        shutil.rmtree(self.work, ignore_errors=True)

    def do_shutdown(self) -> None:
        self._remove_work()
        Gtk.Application.do_shutdown(self)

    def _clear_results(self) -> None:
        self.strip.remove_all()
        self.strip_heading.set_visible(False)
        child = self.detail.get_first_child()
        while child is not None:
            nxt = child.get_next_sibling()
            self.detail.remove(child)
            child = nxt

    def on_primary(self) -> None:
        name = self.stack.get_visible_child_name()
        if name == "device":
            self.load_bundles()
            self._go("bundle")
        elif name == "bundle":
            self._go("size")
        elif name == "size":
            self.refresh_estimate()
            self._go("estimate")
        elif name == "estimate":
            self.start_scan()
        elif name == "results":
            self.choose_another()

    def choose_another(self) -> None:
        if self.scan_thread is not None and self.scan_thread.is_alive():
            return
        if not self.delete_result():
            return
        self.input_path = None
        self.scan_thread = None
        self._clear_results()
        self.summary.set_text("")
        self.context_note.set_text("")
        self.result_note.set_text("")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self.file_label.set_text("")
        self.warn_label.set_text("")
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
        if len(self.history) <= 1:
            return
        self.history.pop()
        self._show(self.history[-1])

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

    def _run_json(self, args: list[str]) -> dict:
        proc = subprocess.run([self.bin, *args], check=False, capture_output=True, text=True)
        if not proc.stdout.strip():
            return {"status": "refused", "message": proc.stderr.strip() or "No response."}
        return json.loads(proc.stdout)

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
            or self.reason.get_text() != "The file was not fully decoded."
            or not self.reason.get_visible()
        ):
            self._exercise_fail(f"incomplete reason missing: {self.summary.get_text()!r} {self.reason.get_text()!r}")
            return False
        self._exercise_report = None
        self._clear_results()
        self.summary.set_text("")
        self.context_note.set_text("")
        self.result_note.set_text("")
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
        self.context_note.set_text("")
        self.result_note.set_text("")
        self.reason.set_text("")
        self.reason.set_visible(False)
        self.choose_file(path)
        aged = time.time() - 40 * 24 * 3600
        os.utime(path, (aged, aged))
        self.choose_file(path)
        if self.warn_label.get_text() != PHRASES["old"]:
            self._exercise_fail(f"old file warning missing: {self.warn_label.get_text()!r}")
            return False
        now = time.time()
        os.utime(path, (now, now))
        self.choose_file(path)
        if self.warn_label.get_text():
            self._exercise_fail(f"fresh file warned: {self.warn_label.get_text()!r}")
            return False
        missing = str(Path(path).with_name("no-such-photo.png"))
        self.choose_file(missing)
        folder = Path(path).with_name("not-a-file-dir")
        folder.mkdir(exist_ok=True)
        self.choose_file(str(folder))
        if (
            self.pick_notice.get_text() != "The file could not be read. Refusing."
            or not self.pick_notice.get_visible()
            or self.input_path != path
            or self.stack.get_visible_child_name() != "device"
        ):
            self._exercise_fail(
                f"an unreadable file changed the pick: {self.pick_notice.get_text()!r} {self.input_path!r}"
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
        self.refresh_estimate()
        self._go("estimate")
        if self.primary.get_visible() or self.primary.get_sensitive() or "Refusing." not in self.estimate_body.get_text():
            self._exercise_fail(f"a bad file offered Analyze: {self.estimate_body.get_text()!r}")
            return False
        if (
            self.missing_button.get_visible()
            or self.wanted_button.get_visible()
            or self.class_label.get_visible()
            or "Choose missing, wanted, or both." in self.estimate_body.get_text()
        ):
            self._exercise_fail("a refused estimate asked for a poster class")
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
        self.bundle_id = "not-in-the-catalog"
        self.load_bundles()
        if self.bundle_id != "fast":
            self._exercise_fail(f"a missing bundle stayed selected: {self.bundle_id}")
            return False
        self._go("bundle")
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
        if "CPU" not in text or "second" not in text.lower() and "Less than" not in text:
            self._exercise_fail(f"estimate missing time or CPU note: {text}")
            return False
        if "Fast. 640 px on the long side. Every decoded frame." not in text or "Bundle fast" in text or "Coverage complete" in text:
            self._exercise_fail(f"estimate did not repeat the choice in plain words: {text}")
            return False
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
        if self.class_label.get_text() != "Missing." or self._class_args() != ["--no-wanted"] or not self.primary.get_sensitive():
            self._exercise_fail(f"wanted off did not leave missing: {self.class_label.get_text()!r} {self._class_args()}")
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
        self.context_note.set_text("Missing and wanted.")
        self.result_note.set_text(PHRASES["clearance"])
        self.reason.set_text("The file was not fully decoded.")
        self.reason.set_visible(True)
        self.start_scan()
        if (
            self.summary.get_text() != "Scanning"
            or self.context_note.get_text()
            or self.result_note.get_text()
            or self.reason.get_text()
            or self.reason.get_visible()
            or self.primary.get_label() != "Scanning"
            or self.delete_button.get_visible()
            or self.primary.get_sensitive()
            or self.back.get_sensitive()
            or self.stack.get_visible_child_name() != "results"
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
        self._add_crop({"kind": "plate", "crop": "crops/plate.png", "label": "No poster publishes a plate."})
        self._add_crop({"kind": "vehicle", "crop": "crops/plate.png", "label": "A vehicle is not a person."})
        self._add_crop({"kind": "audio", "crop": "crops/plate.png", "label": "skip me"})
        self._add_crop({"kind": "face", "label": "Not compared."})
        live = self._strip_labels()
        self.out_dir = saved_out
        if (
            "No poster publishes a plate." not in live
            or "A vehicle is not a person." not in live
            or "skip me" in live
            or "Not compared." in live
            or not self.strip_heading.get_visible()
        ):
            self._exercise_fail(f"a plate or a vehicle stayed off the running scan: {live}")
            return False
        self.strip.remove_all()
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
        if "Missing and wanted." not in self.context_note.get_text():
            self._exercise_fail(f"the result did not name the classes under the headline: {self.context_note.get_text()!r}")
            return False
        result_shot = os.environ.get("OPENWORLD_RESULT_SHOT")
        if result_shot and not getattr(self, "_result_shot_saved", False):
            self._result_shot_saved = True
            subprocess.run(
                ["xdotool", "search", "--name", "^OpenWorld$", "windowmove", "40", "40"],
                check=False,
            )
            self._grab(result_shot)
        if report.get("status") != "complete":
            self._exercise_fail(report.get("message", "not complete"))
            return False
        labels = self._strip_labels()
        for phrase in (PHRASES["possible"], PHRASES["not_compared"], "A vehicle is not a person."):
            if phrase not in labels:
                self._exercise_fail(f"strip missing {phrase}: {labels}")
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
        if card_text.count("Crop") < expected or card_text.count("Frame") < expected:
            self._exercise_fail(f"a candidate card is missing its crop or frame: {card_text}")
            return False
        if "Score " not in card_text or "Cosine" in card_text or "FIX123" not in card_text:
            self._exercise_fail(f"the card did not explain the score and the plate: {card_text}")
            return False
        if self.strip_heading.get_text() != "Crops from this file." or not self.strip_heading.get_visible():
            self._exercise_fail("the other crops are not labeled under the cards")
            return False
        result = self.out_dir
        if result is None or not (result / "result.json").is_file() or not self.delete_button.get_visible():
            self._exercise_fail("no result to delete")
            return False
        headline = self.summary.get_text()
        saved = (result / "result.json").read_bytes()
        (result / "result.json").unlink()
        if (
            self.delete_result()
            or self.summary.get_text() != headline
            or "not an OpenWorld result" not in self.delete_notice.get_text()
            or not self.delete_notice.get_visible()
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
            or self.result_note.get_text()
            or self.delete_button.get_visible()
            or self.delete_notice.get_visible()
        ):
            self._exercise_fail("result remained after delete")
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
        if self.stack.get_visible_child_name() != "choose" or self.input_path is not None:
            self._exercise_fail("choose another did not return to the start")
            return False
        Path(os.environ.get("OPENWORLD_STATUS", "/tmp/openworld-exercise.json")).write_text(
            json.dumps({"ok": True, "summary": report.get("summary"), "work": str(self.work)})
        )
        try:
            self._save_shot()
        finally:
            self.quit()
        return False

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
