#!/usr/bin/env python3
"""Download the weight files a bundle pins, and keep only the bytes that match.

A file whose SHA-256 differs from the manifest is deleted. Nothing is trusted
because it came from a known address. Files without a url are skipped: those
weights are not redistributable, so no address is published for them.
"""

import argparse
import hashlib
import sys
import tomllib
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def fetch(bundle: Path) -> int:
    manifest = tomllib.loads((bundle / "manifest.toml").read_text())
    failures = 0
    for pin in manifest.get("files", []):
        url = pin.get("url")
        want = pin.get("sha256", "").strip()
        if not url or not want:
            continue
        target = bundle / pin["path"]
        if target.is_file() and sha256(target) == want:
            print(f"{manifest['id']} {pin['role']}: already pinned")
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        partial = target.with_suffix(target.suffix + ".part")
        with urllib.request.urlopen(url, timeout=120) as response, partial.open("wb") as out:
            while block := response.read(1 << 20):
                out.write(block)
        got = sha256(partial)
        if got != want:
            partial.unlink()
            print(f"{manifest['id']} {pin['role']}: SHA-256 {got} is not the pin {want}. Refusing.", file=sys.stderr)
            failures += 1
            continue
        partial.replace(target)
        print(f"{manifest['id']} {pin['role']}: pinned {want}")
    return failures


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundles", default=str(ROOT / "bundles"))
    parser.add_argument("bundle", nargs="*", help="bundle ids; every bundle with a url when empty")
    args = parser.parse_args()
    root = Path(args.bundles)
    ids = args.bundle or sorted(p.name for p in root.iterdir() if (p / "manifest.toml").is_file())
    failures = sum(fetch(root / bundle_id) for bundle_id in ids)
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
