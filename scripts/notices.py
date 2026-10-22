#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Write docs/legal/third-party.txt: every third-party component the apps ship, with its license.

    python3 scripts/notices.py           # write
    python3 scripts/notices.py --check   # fail when the file is out of date

The core embeds the file and the apps show it under Licenses. It lists the models, ONNX
Runtime (its license and notices, checked against core/openworld-core/onnxruntime.pins),
the desktop and Android libraries, and every Rust crate a shipped program links: the
normal dependencies of the core and desktop workspaces on every platform, each with the
license files the crate itself ships. Identical texts are printed once, with every crate
that uses them. `cargo metadata` downloads the crates when they are not cached.
"""
import hashlib
import json
import subprocess
import sys
import tomllib
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs/legal/third-party.txt"
TEXTS = ROOT / "docs/legal/licenses"
PINS = ROOT / "core/openworld-core/onnxruntime.pins"
WORKSPACES = ("core/Cargo.toml", "desktop/Cargo.toml")
LICENSE_NAMES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "COPYRIGHT")
RULE = "=" * 48

# What is not a Rust crate. The texts are committed under docs/legal/licenses.
MODELS = [
    ("YuNet 2023mar face detector, OpenCV Zoo", "MIT", "https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet", "yunet-LICENSE.txt"),
    ("SFace 2021dec face recognizer, OpenCV Zoo", "Apache-2.0", "https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface", "sface-LICENSE.txt"),
]
ORT_FILES = ("onnxruntime-LICENSE.txt", "onnxruntime-ThirdPartyNotices.txt")
DESKTOP = [
    ("FFmpeg (ffmpeg and ffprobe, which decode files on computers)",
     "LGPL-2.1-or-later in the Linux Flatpak. The Windows folder copies MSYS2's FFmpeg, which MSYS2 builds as GPL-3.0-or-later; "
     "the Windows folder carries each MSYS2 package's own license files under share/licenses.",
     "https://ffmpeg.org/legal.html", ("LGPL-2.1", "GPL-3")),
    ("GTK 4, libadwaita, GLib, GdkPixbuf, Pango, and the libraries they load (the desktop app)",
     "LGPL-2.1-or-later for GTK, libadwaita, GLib, GdkPixbuf, and Pango; each other library under its own license, "
     "carried with it by the Flatpak runtime and, on Windows, under share/licenses.",
     "https://www.gtk.org", ("LGPL-2.1",)),
    ("Adwaita icon theme (the desktop app's icons)",
     "LGPL-3.0, or Creative Commons Attribution-Share Alike 3.0 United States, at your choice.",
     "https://gitlab.gnome.org/GNOME/adwaita-icon-theme", ("LGPL-3",)),
]
# The full texts the desktop entries name, as Debian and Ubuntu ship them.
COMMON = {"LGPL-2.1": "LGPL-2.1.txt", "GPL-3": "GPL-3.txt", "LGPL-3": "LGPL-3.txt"}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def ort_pins() -> tuple[str, dict]:
    """The pinned ONNX Runtime version and the SHA-256 of each license file it ships."""
    version, shas = None, {}
    for line in PINS.read_text().splitlines():
        fields = line.split()
        if len(fields) != 5 or line.startswith("#"):
            continue
        _, kind, sha, name, _ = fields
        if kind == "wheel" and version is None:
            version = name.split("-")[1]
        if name in ORT_FILES:
            shas.setdefault(name, set()).add(sha)
    return version, shas


def ort_texts() -> tuple[str, list]:
    """ONNX Runtime's license and notices, copied from a fetched pin when one is there."""
    version, shas = ort_pins()
    texts = []
    for name in ORT_FILES:
        kept = TEXTS / name
        fetched = sorted((ROOT / "third_party/onnxruntime").glob(f"*/{name}"))
        if fetched and sha256(fetched[0]) in shas[name] and "--check" not in sys.argv:
            kept.write_bytes(fetched[0].read_bytes())
        if not kept.is_file() or sha256(kept) not in shas[name]:
            raise SystemExit(f"{kept.relative_to(ROOT)} is not ONNX Runtime's pinned {name}. Run scripts/fetch_ort.py, then this.")
        texts.append(kept.read_text())
    return version, texts


def android_libraries() -> list:
    catalog = tomllib.loads((ROOT / "android/gradle/libs.versions.toml").read_text())
    versions, libraries = catalog["versions"], catalog["libraries"]
    used = [line.split("libs.")[1].split(")")[0].replace(".", "-")
            for line in (ROOT / "android/app/build.gradle.kts").read_text().splitlines()
            if line.strip().startswith("implementation(")]
    rows = []
    for alias in used:
        entry = libraries[alias]
        ref = entry.get("version", {}).get("ref")
        version = versions[ref] if ref else f"(Compose BOM {versions['composeBom']})"
        license = "MIT" if entry["module"].startswith("com.microsoft.onnxruntime") else "Apache-2.0"
        rows.append(f"{entry['module']} {version}: {license}")
    return rows


def crates() -> list:
    """(name, version, license, repository, [license file paths]) for every shipped crate."""
    found = {}
    for workspace in WORKSPACES:
        meta = json.loads(subprocess.run(
            ["cargo", "metadata", "--manifest-path", str(ROOT / workspace), "--format-version", "1", "--locked"],
            check=True, capture_output=True, text=True).stdout)
        packages = {p["id"]: p for p in meta["packages"]}
        nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
        members = set(meta["workspace_members"])
        seen, todo = set(), list(members)
        while todo:
            node = nodes[todo.pop()]
            for dep in node["deps"]:
                if any(kind["kind"] is None for kind in dep["dep_kinds"]) and dep["pkg"] not in seen:
                    seen.add(dep["pkg"])
                    todo.append(dep["pkg"])
        for pid in seen - members:
            p = packages[pid]
            if p["source"] is None:  # a path dependency: OpenWorld's own code
                continue
            folder = Path(p["manifest_path"]).parent
            files = sorted(f for f in folder.iterdir() if f.is_file() and f.name.upper().startswith(LICENSE_NAMES))
            found[(p["name"], p["version"])] = (p["name"], p["version"], p.get("license") or "see its license file",
                                                 p.get("repository") or "", files)
    return [found[k] for k in sorted(found)]


def render() -> str:
    out = ["OpenWorld third-party software and models", "",
           "OpenWorld is published by OneX Engineering under the Apache License 2.0. It uses the",
           "software and models below, each under its own license, reproduced here.", ""]

    def section(title):
        out.extend([RULE, title, RULE, ""])

    section("Models")
    for name, license, url, text in MODELS:
        out.extend([f"{name}", f"License: {license}", url, "", (TEXTS / text).read_text().rstrip(), ""])

    version, texts = ort_texts()
    section(f"ONNX Runtime {version} (Microsoft), which runs the models")
    out.extend(["License: MIT", "https://github.com/microsoft/onnxruntime", ""])
    for text in texts:
        out.extend([text.rstrip(), ""])

    section("Computers: FFmpeg and the GTK desktop")
    named = []
    for name, license, url, wanted in DESKTOP:
        out.extend([name, f"License: {license}", url, ""])
        named.extend(w for w in wanted if w not in named)
    for key in named:
        out.extend([f"{key} text:", "", (TEXTS / COMMON[key]).read_text().rstrip(), ""])

    section("Android")
    out.append("The Android app uses these libraries and the AndroidX, Jetpack Compose, and Kotlin libraries they depend on:")
    out.append("")
    out.extend(android_libraries())
    out.extend(["", "Apache-2.0 is reproduced in full at the end of the Rust crates section below.", ""])

    section("Rust crates in the core and the desktop app")
    by_text = defaultdict(list)
    bare = []
    for name, version, license, repo, files in crates():
        out.append(f"{name} {version}: {license}" + (f" ({repo})" if repo else ""))
        if not files:
            # Only MIT and Apache-2.0 have a standard text below.
            if not any(known in license for known in ("MIT", "Apache-2.0")):
                raise SystemExit(f"{name} {version} ships no license file and is {license}. Add its text to this script.")
            bare.append(f"{name} {version}")
        for f in files:
            by_text[f.read_text(errors="replace").strip()].append(f"{name} {version} ({f.name})")
    out.append("")
    for text, users in sorted(by_text.items(), key=lambda item: item[1][0]):
        out.extend(["-" * 48, "Used by: " + ", ".join(users), "", text, ""])
    if bare:
        out.extend(["-" * 48, "These crates ship no license file. Each is under the license named above, whose standard text follows:",
                    ", ".join(bare), ""])
    out.extend(["-" * 48, "Apache License 2.0 (standard text)", "", (ROOT / "LICENSE").read_text().rstrip(), ""])
    out.extend(["-" * 48, "MIT License (standard text)", "", (TEXTS / "MIT.txt").read_text().rstrip(), ""])
    return "\n".join(out).rstrip() + "\n"


def main():
    text = render()
    if "--check" in sys.argv[1:]:
        if not OUT.is_file() or OUT.read_text() != text:
            raise SystemExit(f"{OUT.relative_to(ROOT)} is out of date. Run scripts/notices.py.")
        return
    OUT.write_text(text)


if __name__ == "__main__":
    main()
