#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Print the UDID of an available iPhone simulator for `xcodebuild -destination id=...`.

An iPhone 16 on the newest iOS runtime that has one, else the newest runtime's first
iPhone. Reads `xcrun simctl list devices available --json`. Used by CI, where the
installed runtimes change with the runner image.
"""

import json
import re
import subprocess
import sys


def version(runtime: str) -> tuple:
    return tuple(int(n) for n in re.findall(r"\d+", runtime.split(".iOS-")[-1]))


def main() -> int:
    listing = subprocess.run(
        ["xcrun", "simctl", "list", "devices", "available", "--json"], check=True, capture_output=True, text=True
    ).stdout
    devices = json.loads(listing)["devices"]
    runtimes = sorted(
        (r for r, ds in devices.items() if ".iOS-" in r and any(d["name"].startswith("iPhone") for d in ds)),
        key=version,
    )
    if not runtimes:
        print("no iPhone simulator is available", file=sys.stderr)
        return 1
    for runtime in reversed(runtimes):
        named = [d for d in devices[runtime] if d["name"] == "iPhone 16"]
        if named:
            break
    else:
        runtime = runtimes[-1]
        named = [d for d in devices[runtime] if d["name"].startswith("iPhone")]
    print(f"{named[0]['name']} on {runtime}", file=sys.stderr)
    print(named[0]["udid"])
    return 0


if __name__ == "__main__":
    sys.exit(main())
