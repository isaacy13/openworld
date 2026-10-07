#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Desktop window for OpenWorld. The scan stays in the Rust library."""

from __future__ import annotations

import json
import os
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
        self.input_path: str | None = None
        self.bundle_id = "fast"
        self.long_side = "640"
        self.coverage = "complete"
        self.rows: list[dict] = []
        self.out_dir: Path | None = None
        self.posters: Path | None = None
        self.scan_thread: threading.Thread | None = None

    def do_activate(self) -> None:
        self.window = Gtk.ApplicationWindow(application=self, title="OpenWorld")
        self.window.set_default_size(980, 700)
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
        self.stack.set_margin_top(18)
        self.stack.set_margin_bottom(18)
        self.stack.set_margin_start(24)
        self.stack.set_margin_end(24)
        self.window.set_child(self.stack)

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
        box.append(title)
        box.append(self.estimate_body)
        return box

    def _results_page(self) -> Gtk.Widget:
        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.summary = Gtk.Label(xalign=0, wrap=True)
        self.summary.add_css_class("title")
        self.result_note = Gtk.Label(xalign=0, wrap=True)
        self.result_note.add_css_class("dim")
        self.strip = Gtk.FlowBox()
        self.strip.set_max_children_per_line(6)
        self.strip.set_selection_mode(Gtk.SelectionMode.NONE)
        self.detail = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        scroll = Gtk.ScrolledWindow()
        inner = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        inner.append(self.summary)
        inner.append(self.result_note)
        inner.append(self.strip)
        inner.append(self.detail)
        scroll.set_child(inner)
        scroll.set_vexpand(True)
        delete = Gtk.Button(label="Delete")
        delete.set_halign(Gtk.Align.START)
        delete.connect("clicked", lambda *_: self.delete_result())
        outer.append(scroll)
        outer.append(delete)
        return outer

    def pick_file(self) -> None:
        dialog = Gtk.FileDialog()
        dialog.open(self.window, None, self._file_chosen)

    def _file_chosen(self, dialog: Gtk.FileDialog, result: Gio.AsyncResult) -> None:
        try:
            chosen = dialog.open_finish(result)
        except GLib.Error:
            return
        if chosen is not None:
            self.choose_file(chosen.get_path())

    def _on_drop(self, _target, value, _x, _y) -> bool:
        path = value.get_path()
        if path:
            self.choose_file(path)
            return True
        return False

    def choose_file(self, path: str) -> None:
        self.input_path = path
        self.file_label.set_text(Path(path).name)
        age = time.time() - os.stat(path).st_mtime
        if age > 30 * 24 * 3600:
            self.warn_label.set_text(PHRASES["old"])
        else:
            self.warn_label.set_text("")
        self._go("device")

    def load_bundles(self) -> None:
        payload = self._run_json(["--json", "--bundles", self.bundles, "bundles"])
        self.rows = payload.get("bundles", [])
        while True:
            row = self.bundle_list.get_row_at_index(0)
            if row is None:
                break
            self.bundle_list.remove(row)
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
            best = Gtk.Label(label=item["best_for"], xalign=0)
            curve = Gtk.Label(label=item["curve_line"], xalign=0)
            curve.add_css_class("dim")
            box.append(name)
            box.append(best)
            box.append(curve)
            row.set_child(box)
            self.bundle_list.append(row)
            if item.get("preselected"):
                selected = row
        if selected is not None:
            self.bundle_list.select_row(selected)

    def _on_bundle_row(self, _list, row) -> None:
        if row is not None:
            self.bundle_id = row.bundle_id

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
            self.primary.set_sensitive(False)
            return
        self.primary.set_sensitive(True)
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
        lines.append(f"Bundle {self.bundle_id}. Detection {self.long_side}. Coverage {self.coverage}.")
        self.estimate_body.set_text("\n".join(line for line in lines if line))

    def start_scan(self) -> None:
        if self.scan_thread is not None:
            return
        self._clear_results()
        self.summary.set_text("Scanning")
        self._go("results")
        self.primary.set_sensitive(False)
        self.posters = self.work / "posters"
        self.out_dir = self.work / "result"
        subprocess.check_call(
            [self.bin, "--bundles", self.bundles, "posters", "write-fixture", "--out", str(self.posters)],
            stdout=subprocess.DEVNULL,
        )
        self.scan_thread = threading.Thread(target=self._scan_worker, daemon=True)
        self.scan_thread.start()

    def _scan_worker(self) -> None:
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
                str(self.out_dir),
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
        if event.get("kind") != "face" or not event.get("crop") or self.out_dir is None:
            return False
        path = self.out_dir / event["crop"]
        if not path.is_file():
            return False
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        picture = Gtk.Picture.new_for_filename(str(path))
        picture.set_size_request(112, 112)
        picture.set_can_shrink(False)
        caption = Gtk.Label(label=event.get("label", ""), wrap=True, justify=Gtk.Justification.CENTER)
        box.append(picture)
        box.append(caption)
        self.strip.append(box)
        return False

    def _show_report(self, report: dict) -> bool:
        self.summary.set_text(report.get("summary") or "")
        notes = []
        if report.get("coverage_banner"):
            notes.append(report["coverage_banner"])
        if report.get("bundle_name"):
            notes.append(f"Bundle: {report['bundle_name']}")
        if report.get("perception_note"):
            notes.append(report["perception_note"])
        for warning in report.get("warnings") or []:
            notes.append(warning)
        for line in report.get("disclosure") or []:
            notes.append(line)
        self.result_note.set_text("\n".join(notes))
        for candidate in report.get("candidates") or []:
            self.detail.append(self._candidate_card(candidate))
        if not report.get("candidates") and report.get("status") == "complete":
            clearance = Gtk.Label(label=PHRASES["clearance"], xalign=0, wrap=True)
            self.detail.append(clearance)
        self.primary.set_sensitive(True)
        self.primary.set_label("Done")
        self._exercise_report = report
        return False

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
            if rel and self.out_dir is not None:
                path = self.out_dir / rel
                if path.is_file():
                    picture = Gtk.Picture.new_for_filename(str(path))
                    picture.set_size_request(160, 120)
                    images.append(picture)
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

    def confirm_leave(self, url: str) -> None:
        payload = self._run_json(["--json", "leave", "--url", url])
        message = payload.get("message", PHRASES["leaving"])
        dialog = Gtk.MessageDialog(
            transient_for=self.window,
            modal=True,
            message_type=Gtk.MessageType.QUESTION,
            buttons=Gtk.ButtonsType.NONE,
            text=message,
        )
        dialog.add_button("Stay", Gtk.ResponseType.CANCEL)
        dialog.add_button("Open", Gtk.ResponseType.ACCEPT)
        dialog.format_secondary_text(payload.get("url", url))
        dialog.connect("response", self._leave_response, payload.get("url", url))
        dialog.present()

    def _leave_response(self, dialog: Gtk.MessageDialog, response: int, url: str) -> None:
        dialog.destroy()
        if response == Gtk.ResponseType.ACCEPT and url.startswith("https://"):
            Gio.AppInfo.launch_default_for_uri(url, None)

    def delete_result(self) -> None:
        if self.out_dir is None:
            return
        subprocess.call([self.bin, "--json", "delete", "--out", str(self.out_dir)])
        self._clear_results()
        self.summary.set_text("Deleted.")
        self.out_dir = None

    def _clear_results(self) -> None:
        self.strip.remove_all()
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
        self.back.set_sensitive(len(self.history) > 1 and name != "results")
        labels = {
            "choose": "Choose File",
            "device": "Continue",
            "bundle": "Continue",
            "size": "Continue",
            "estimate": "Analyze",
            "results": "Done",
        }
        self.primary.set_label(labels.get(name, "Continue"))
        self.primary.set_visible(name != "choose")

    def _run_json(self, args: list[str]) -> dict:
        proc = subprocess.run([self.bin, *args], check=False, capture_output=True, text=True)
        if not proc.stdout.strip():
            return {"status": "refused", "message": proc.stderr.strip() or "No response."}
        return json.loads(proc.stdout)

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
        self.choose_file(path)
        self.load_bundles()
        self._go("bundle")
        self.long_side = "640"
        self.coverage = "complete"
        self.refresh_estimate()
        self._go("estimate")
        text = self.estimate_body.get_text()
        if "CPU" not in text or "second" not in text.lower() and "Less than" not in text:
            self._exercise_fail(f"estimate missing time or CPU note: {text}")
            return False
        self.start_scan()
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
        if report.get("status") != "complete":
            self._exercise_fail(report.get("message", "not complete"))
            return False
        Path(os.environ.get("OPENWORLD_STATUS", "/tmp/openworld-exercise.json")).write_text(
            json.dumps({"ok": True, "summary": report.get("summary")})
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
        path = os.environ.get("OPENWORLD_SHOT")
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
