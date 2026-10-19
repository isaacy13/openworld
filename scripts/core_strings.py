#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Write the Apple string catalog of core sentences from the core's own `copy` answer.

    python3 scripts/core_strings.py core/target/release/openworld          # write
    python3 scripts/core_strings.py core/target/release/openworld --check  # fail on drift
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "apple/App/Resources/Core.xcstrings"
COMMENT = "A core sentence, looked up by its code (docs/api.md). Keep the meaning exact."


def catalog(binary: str) -> str:
    answer = subprocess.run([binary, "--bundles", str(ROOT / "bundles"), "--json", "copy"], check=True, capture_output=True, text=True)
    copy = json.loads(answer.stdout)["copy"]
    strings = {
        code: {
            "comment": COMMENT,
            "extractionState": "manual",
            "localizations": {"en": {"stringUnit": {"state": "translated", "value": text}}},
        }
        for code, text in sorted(copy.items())
    }
    return json.dumps({"sourceLanguage": "en", "strings": strings, "version": "1.0"}, indent=2, ensure_ascii=False) + "\n"


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    text = catalog(sys.argv[1])
    if "--check" in sys.argv[2:]:
        if CATALOG.read_text() != text:
            raise SystemExit(f"{CATALOG.relative_to(ROOT)} is out of date. Run scripts/core_strings.py.")
        return
    CATALOG.write_text(text)


if __name__ == "__main__":
    main()
