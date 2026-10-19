#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Pack one release archive: programs, the pinned ONNX Runtime, and the bundle catalog.

    python3 scripts/package_release.py --name openworld-cli-v0.1.0-linux-x86_64 \
        --platform linux-x86_64 --program core/target/release/openworld --out dist

The layout is the one the core and the apps look for:

    <name>/openworld[.exe]          the program(s)
    <name>/<ONNX Runtime library>   next to the program, checked against its pin at load
    <name>/onnxruntime-*.txt        ONNX Runtime's license and notices
    <name>/bundles/*/manifest.toml  the catalog (and curve.json where one exists)
    <name>/LICENSE, README.md

Weights are not packed (`scripts/fetch_weights.py`). Every ONNX Runtime file is checked
against core/openworld-core/onnxruntime.pins again here, so a stale third_party/ folder
cannot reach a release. Entries are sorted and dated SOURCE_DATE_EPOCH (or 0) with no
owner, so the same inputs give the same archive bytes. Prints the archive path.
"""

import argparse
import gzip
import io
import os
import shutil
import sys
import tarfile
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import fetch_ort  # noqa: E402

ROOT = fetch_ort.ROOT


def stage(name: str, platform: str, programs: list, ort_root: Path, staging: Path) -> Path:
    top = staging / name
    if top.exists():
        shutil.rmtree(top)
    top.mkdir(parents=True)
    for program in programs:
        program = Path(program)
        if not program.is_file():
            raise SystemExit(f"{program} is not built. Refusing.")
        shutil.copy2(program, top / program.name)
    pins = fetch_ort.read_pins()
    if platform not in pins:
        raise SystemExit(f"No pinned ONNX Runtime for {platform}. Refusing.")
    for kind, sha, leaf, _ in pins[platform]["files"]:
        source = ort_root / platform / leaf
        if not source.is_file() or fetch_ort.sha256_file(source) != sha:
            raise SystemExit(f"{source} is not the {kind} pin {sha}. Run scripts/fetch_ort.py {platform}. Refusing.")
        shutil.copy2(source, top / leaf)
    for manifest in sorted((ROOT / "bundles").glob("*/manifest.toml")):
        folder = top / "bundles" / manifest.parent.name
        folder.mkdir(parents=True)
        shutil.copy2(manifest, folder / "manifest.toml")
        curve = manifest.parent / "curve.json"
        if curve.is_file():
            shutil.copy2(curve, folder / "curve.json")
    for doc in ("LICENSE", "README.md"):
        shutil.copy2(ROOT / doc, top / doc)
    return top


def files_under(top: Path) -> list:
    return sorted(p for p in top.rglob("*") if p.is_file())


def write_tar_gz(top: Path, out: Path, epoch: int) -> None:
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as archive:
        for path in files_under(top):
            info = tarfile.TarInfo(str(path.relative_to(top.parent)).replace(os.sep, "/"))
            info.size = path.stat().st_size
            info.mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
            info.mtime = epoch
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            with path.open("rb") as handle:
                archive.addfile(info, handle)
    with out.open("wb") as handle, gzip.GzipFile(fileobj=handle, mode="wb", mtime=epoch, filename="") as gz:
        gz.write(raw.getvalue())


def write_zip(top: Path, out: Path, epoch: int) -> None:
    import time

    stamp = time.gmtime(max(epoch, 315532800))[:6]  # zip cannot date before 1980
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in files_under(top):
            info = zipfile.ZipInfo(str(path.relative_to(top.parent)).replace(os.sep, "/"), stamp)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o755 if path.suffix in (".exe", ".dll") else 0o644) << 16
            archive.writestr(info, path.read_bytes())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--name", required=True, help="archive and top folder name")
    parser.add_argument("--platform", required=True, help="an onnxruntime.pins platform")
    parser.add_argument("--program", action="append", required=True, help="a built program; repeatable")
    parser.add_argument("--ort", default=str(ROOT / "third_party" / "onnxruntime"))
    parser.add_argument("--out", required=True, help="folder for the archive")
    parser.add_argument("--format", choices=("tar.gz", "zip"), default=None, help="zip on Windows, tar.gz elsewhere")
    parser.add_argument("--keep", action="store_true", help="keep the staged folder beside the archive")
    args = parser.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    fmt = args.format or ("zip" if args.platform.startswith("windows") else "tar.gz")
    staging = out / ".staging"
    top = stage(args.name, args.platform, args.program, Path(args.ort), staging)
    archive = out / f"{args.name}.{fmt}"
    (write_zip if fmt == "zip" else write_tar_gz)(top, archive, epoch)
    if args.keep:
        shutil.move(str(top), str(out / args.name))
    shutil.rmtree(staging, ignore_errors=True)
    print(archive)
    return 0


if __name__ == "__main__":
    sys.exit(main())
