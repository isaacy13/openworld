#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""A CycloneDX SBOM of the Android build, from android/gradle/verification-metadata.xml.

That file is Gradle's own record of every artifact the build resolves (the app's
libraries and the build's plugins and tools), each with the SHA-256 Gradle enforces.
This turns it into CycloneDX 1.5 JSON, one component per module, with the SHA-256 of
its jars and AARs. It lists what the build may resolve, build tools included, which is
more than what ships in the APK. No plugin runs inside the build to make it.

    python3 scripts/gradle_sbom.py --version v0.1.0 --out openworld-android.cdx.json
"""

import argparse
import hashlib
import json
import sys
import uuid
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NS = "{https://schema.gradle.org/dependency-verification}"
METADATA = (".pom", ".module")


def components(metadata: Path) -> list:
    tree = ET.parse(metadata)
    out = []
    for component in tree.getroot().iter(f"{NS}component"):
        group, name, version = component.get("group"), component.get("name"), component.get("version")
        hashes, files = [], []
        for artifact in component.findall(f"{NS}artifact"):
            filename = artifact.get("name")
            files.append(filename)
            if filename.endswith(METADATA):
                continue
            for sha in artifact.findall(f"{NS}sha256"):
                hashes.append({"alg": "SHA-256", "content": sha.get("value")})
        entry = {
            "type": "library",
            "bom-ref": f"pkg:maven/{group}/{name}@{version}",
            "group": group,
            "name": name,
            "version": version,
            "purl": f"pkg:maven/{group}/{name}@{version}",
            "properties": [{"name": "gradle:artifact", "value": f} for f in sorted(files)],
        }
        if hashes:
            entry["hashes"] = sorted(hashes, key=lambda h: h["content"])
        out.append(entry)
    return sorted(out, key=lambda c: c["bom-ref"])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--metadata", default=str(ROOT / "android" / "gradle" / "verification-metadata.xml"))
    parser.add_argument("--version", required=True, help="the release version")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    metadata = Path(args.metadata)
    if not metadata.is_file():
        raise SystemExit(f"{metadata} is missing. Gradle dependency verification is not set up. Refusing.")
    listed = components(metadata)
    body = metadata.read_bytes()
    bom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        # The same metadata gives the same serial number.
        "serialNumber": f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, f'openworld-android:{args.version}:{hashlib.sha256(body).hexdigest()}')}",
        "version": 1,
        "metadata": {
            "component": {"type": "application", "bom-ref": "openworld-android", "name": "openworld-android", "version": args.version},
            "properties": [{"name": "source", "value": "android/gradle/verification-metadata.xml"}],
        },
        "components": listed,
    }
    Path(args.out).write_text(json.dumps(bom, indent=2) + "\n")
    print(f"{args.out}: {len(listed)} components")
    return 0


if __name__ == "__main__":
    sys.exit(main())
