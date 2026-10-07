#!/usr/bin/env python3
"""Shells must keep the product sentences, and must not claim an identification."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = [
    "Possible candidate. Not an identification.",
    "Not compared.",
    "Incomplete.",
    "No candidate is not a clearance.",
    "You are leaving OpenWorld.",
]
SHELLS = [
    ROOT / "desktop",
    ROOT / "apple",
    ROOT / "android",
]
BANNED = ["Identified", "more accurate", "found this person"]


def main() -> None:
    for shell in SHELLS:
        text = "\n".join(path.read_text(errors="replace") for path in shell.rglob("*") if path.is_file())
        for phrase in REQUIRED:
            if phrase not in text:
                raise SystemExit(f"{shell.name} is missing {phrase!r}")
        for phrase in BANNED:
            if phrase in text:
                raise SystemExit(f"{shell.name} contains {phrase!r}")
    readme = (ROOT / "README.md").read_text()
    for phrase in REQUIRED:
        if phrase not in readme:
            raise SystemExit(f"README is missing {phrase!r}")
    print("words ok")


if __name__ == "__main__":
    main()
