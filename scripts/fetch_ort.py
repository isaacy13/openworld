#!/usr/bin/env python3
"""Download the ONNX Runtime shared library the core loads, and keep only pinned bytes.

The wheel is fetched from PyPI and kept only when its SHA-256 matches the pin
below (the digest PyPI publishes for that file). The library inside it is
copied to third_party/onnxruntime/<platform>/. Nothing is trusted because it
came from a known address.

The core finds the library through OPENWORLD_ORT_LIB, then next to the
program, then in that folder (see core/openworld-core/src/runtime.rs).
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

# platform: (wheel file name, sha256 published by PyPI, library inside the wheel)
PINS = {
    "linux-x86_64": (
        "onnxruntime-1.28.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl",
        "0a83bdb70d143cede762b677789bf2a7acca54b3fb82565601d5c30695aa933c",
        "libonnxruntime.so",
    ),
    "linux-aarch64": (
        "onnxruntime-1.28.0-cp312-cp312-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl",
        "4e81a23df16e7acb9d51b06d30cc098e49315ef9180f97bc2221d167b4b04d9c",
        "libonnxruntime.so",
    ),
    "windows-x86_64": (
        "onnxruntime-1.28.0-cp312-cp312-win_amd64.whl",
        "c35064f9b3c43c81c5d5d282091401d0f1ff22796d93ccade4ea2ece5e137ab8",
        "onnxruntime.dll",
    ),
    "windows-x86_64-directml": (
        "onnxruntime_directml-1.24.4-cp312-cp312-win_amd64.whl",
        "f2ecb68b7b7b259d2ef3112ae760149f9b5a1e7c0fbb73d539da6250a648a614",
        "onnxruntime.dll",
    ),
    "macos-arm64": (
        "onnxruntime-1.28.0-cp312-cp312-macosx_14_0_arm64.whl",
        "26ff0fdd06efb6c155bae95387a09db1a2be89c7a03e4d0bffd5a171cc2826da",
        "libonnxruntime.dylib",
    ),
}


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


def fetch(name: str, out_root: Path) -> Path:
    wheel, want, library = PINS[name]
    target_dir = out_root / name
    target = target_dir / library
    stamp = target_dir / "wheel.sha256"
    if target.is_file() and stamp.is_file() and stamp.read_text().strip() == want:
        print(f"{name}: already pinned")
        return target
    with urllib.request.urlopen(wheel_url(wheel), timeout=300) as response:
        body = response.read()
    got = hashlib.sha256(body).hexdigest()
    if got != want:
        raise SystemExit(f"{name}: SHA-256 {got} is not the pin {want}. Refusing.")
    target_dir.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(io.BytesIO(body)) as archive:
        members = [m for m in archive.namelist() if m.startswith("onnxruntime/capi/")]
        wanted = [m for m in members if Path(m).name.startswith(library.split(".")[0]) or "providers_shared" in m]
        if not any(Path(m).name.startswith(library) for m in wanted):
            raise SystemExit(f"{name}: {library} is not in {wheel}. Refusing.")
        for member in wanted:
            data = archive.read(member)
            leaf = Path(member).name
            # The Linux wheel names the file with its version. The loader asks for the plain name.
            if leaf.startswith("libonnxruntime.so."):
                leaf = "libonnxruntime.so"
            elif leaf.startswith("libonnxruntime.") and leaf.endswith(".dylib") and leaf != "libonnxruntime.dylib":
                leaf = "libonnxruntime.dylib"
            (target_dir / leaf).write_bytes(data)
    stamp.write_text(want + "\n")
    print(f"{name}: pinned {want}")
    return target


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", nargs="?", default=None, help=f"one of {', '.join(PINS)}; the host when empty")
    parser.add_argument("--out", default=str(ROOT / "third_party" / "onnxruntime"))
    parser.add_argument("--print-path", action="store_true", help="print the library path on the last line")
    args = parser.parse_args()
    name = args.platform or host_platform()
    if name not in PINS:
        raise SystemExit(f"No pinned ONNX Runtime for {name}. Refusing.")
    path = fetch(name, Path(args.out))
    if args.print_path:
        print(path)


if __name__ == "__main__":
    sys.exit(main())
