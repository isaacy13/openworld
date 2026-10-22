#!/usr/bin/env python3
"""Download the ONNX Runtime shared library the core loads, and keep only pinned bytes.

Every pin is in core/openworld-core/onnxruntime.pins, the file the core's
build script reads too. The wheel is fetched from PyPI and kept only when its
SHA-256 matches the wheel pin (the digest PyPI publishes for that file). Each
file taken out of it is kept only when its own SHA-256 matches its pin, and is
written to third_party/onnxruntime/<platform>/. Nothing is trusted because it
came from a known address.

The core finds the library through OPENWORLD_ORT_LIB, then next to the
program, then in that folder (see core/openworld-core/src/runtime.rs), and
refuses it unless its SHA-256 is the `library` pin for the platform it was
built for.
"""

import argparse
import hashlib
import io
import json
import platform
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PINS_FILE = ROOT / "core" / "openworld-core" / "onnxruntime.pins"
KINDS = ("wheel", "library", "companion")


def read_pins(path: Path = PINS_FILE) -> dict:
    """{platform: {"wheel": (sha, name), "files": [(kind, sha, name, member), ...]}}"""
    pins: dict = {}
    for number, raw in enumerate(path.read_text().splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        fields = line.split()
        if len(fields) != 5 or fields[1] not in KINDS:
            raise SystemExit(f"{path.name}:{number}: expected 'platform kind sha256 name member'. Refusing.")
        name, kind, sha, leaf, member = fields
        if len(sha) != 64 or any(c not in "0123456789abcdef" for c in sha):
            raise SystemExit(f"{path.name}:{number}: {sha} is not a lowercase SHA-256. Refusing.")
        entry = pins.setdefault(name, {"wheel": None, "files": []})
        if kind == "wheel":
            if entry["wheel"] is not None:
                raise SystemExit(f"{path.name}:{number}: {name} has two wheels. Refusing.")
            entry["wheel"] = (sha, leaf)
        else:
            entry["files"].append((kind, sha, leaf, member))
    for name, entry in pins.items():
        libraries = [f for f in entry["files"] if f[0] == "library"]
        if entry["wheel"] is None or len(libraries) != 1:
            raise SystemExit(f"{path.name}: {name} needs one wheel and one library line. Refusing.")
    return pins


def host_platform() -> str:
    machine = platform.machine().lower()
    arch = {"amd64": "x86_64", "x86_64": "x86_64", "aarch64": "aarch64", "arm64": "arm64"}.get(machine, machine)
    system = {"linux": "linux", "windows": "windows", "darwin": "macos"}[platform.system().lower()]
    return f"{system}-{arch}"


def wheel_url(wheel: str) -> str:
    # PyPI's JSON API names the canonical download address for each file.
    project = wheel.split("-")[0].replace("_", "-")
    version = wheel.split("-")[1]
    with urllib.request.urlopen(f"https://pypi.org/pypi/{project}/{version}/json", timeout=60) as response:
        for entry in json.load(response)["urls"]:
            if entry["filename"] == wheel:
                return entry["url"]
    raise SystemExit(f"{wheel} is not listed on PyPI. Refusing.")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def library_path(name: str, out_root: Path, pins: dict) -> Path:
    leaf = next(f[2] for f in pins[name]["files"] if f[0] == "library")
    return out_root / name / leaf


def fetch(name: str, out_root: Path, pins: dict) -> Path:
    want_wheel, wheel = pins[name]["wheel"]
    files = pins[name]["files"]
    target_dir = out_root / name
    target = library_path(name, out_root, pins)
    if all((target_dir / leaf).is_file() and sha256_file(target_dir / leaf) == sha for _, sha, leaf, _ in files):
        print(f"{name}: already pinned")
        return target
    with urllib.request.urlopen(wheel_url(wheel), timeout=300) as response:
        body = response.read()
    got = hashlib.sha256(body).hexdigest()
    if got != want_wheel:
        raise SystemExit(f"{name}: wheel SHA-256 {got} is not the pin {want_wheel}. Refusing.")
    with zipfile.ZipFile(io.BytesIO(body)) as archive:
        extracted = []
        for kind, want, leaf, member in files:
            try:
                data = archive.read(member)
            except KeyError:
                raise SystemExit(f"{name}: {member} is not in {wheel}. Refusing.") from None
            got = hashlib.sha256(data).hexdigest()
            if got != want:
                raise SystemExit(f"{name}: {member} SHA-256 {got} is not the {kind} pin {want}. Refusing.")
            extracted.append((leaf, data))
    # Written only after every file matched, so a refusal leaves nothing half-done.
    target_dir.mkdir(parents=True, exist_ok=True)
    for leaf, data in extracted:
        (target_dir / leaf).write_bytes(data)
    print(f"{name}: pinned {next(f[1] for f in files if f[0] == 'library')}")
    return target


def main() -> None:
    pins = read_pins()
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("platform", nargs="?", default=None, help=f"one of {', '.join(pins)}; the host when empty")
    parser.add_argument("--all", action="store_true", help="fetch and check every pinned platform")
    parser.add_argument("--out", default=str(ROOT / "third_party" / "onnxruntime"))
    parser.add_argument("--print-path", action="store_true", help="print the library path on the last line")
    args = parser.parse_args()
    names = list(pins) if args.all else [args.platform or host_platform()]
    for name in names:
        if name not in pins:
            raise SystemExit(f"No pinned ONNX Runtime for {name}. Refusing.")
    paths = [fetch(name, Path(args.out), pins) for name in names]
    if args.print_path:
        print(paths[-1])


if __name__ == "__main__":
    sys.exit(main())
