#!/usr/bin/env python3
"""Turn a Fast fixture measurement into FMR / FNMR with confidence intervals.

The Rust CLI runs the scan. This script only counts. It does not detect faces.
Real FBI photos stay off: the curve sets real_posters_allowed to false.
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import sys
from pathlib import Path


def wilson(count: int, trials: int, z: float = 1.96) -> tuple[float, float]:
    if trials <= 0:
        raise SystemExit("a rate needs at least one trial")
    p = count / trials
    z2 = z * z
    den = 1.0 + z2 / trials
    center = p + z2 / (2.0 * trials)
    margin = z * math.sqrt(p * (1.0 - p) / trials + z2 / (4.0 * trials * trials))
    lo = max(0.0, (center - margin) / den)
    hi = min(1.0, (center + margin) / den)
    return lo, hi


def rate(count: int, trials: int) -> dict:
    lo, hi = wilson(count, trials)
    return {
        "count": count,
        "trials": trials,
        "rate": count / trials,
        "ci95": [lo, hi],
    }


def sweep(genuine: list[float], impostor: list[float], step: float = 0.05) -> list[dict]:
    points = []
    t = 0.0
    while t <= 1.0001:
        fmr = sum(score >= t for score in impostor)
        fnmr = sum(score < t for score in genuine)
        points.append(
            {
                "threshold": round(t, 2),
                "fmr": fmr / len(impostor),
                "fnmr": fnmr / len(genuine),
            }
        )
        t += step
    return points


def curve_from_measurement(measurement: dict) -> dict:
    genuine = [float(x) for x in measurement["genuine_cosines"]]
    impostor = [float(x) for x in measurement["impostor_cosines"]]
    if not genuine or not impostor:
        raise SystemExit("measurement has no comparison scores")
    threshold = float(measurement["threshold"])
    fmr_count = sum(score >= threshold for score in impostor)
    fnmr_count = sum(score < threshold for score in genuine)
    det_hit = int(measurement["detection_hit"])
    det_miss = int(measurement["detection_miss"])
    return {
        "schema": "openworld.curve.v1",
        "bundle_id": "fast",
        "perception": measurement["perception"],
        "detector_declared": measurement["detector_declared"],
        "embedder_declared": measurement["embedder_declared"],
        "threshold": threshold,
        "hardware": measurement["hardware"],
        "preprocess": measurement["preprocess"],
        "real_posters_allowed": False,
        "picker_line": "Fixture curve measured. Real FBI photos stay off.",
        "note": (
            "This curve is the fixture-marker pipeline on this build's preprocess and CPU. "
            "It is not SCRFD, not ArcFace on photographs, not LFW, and not NIST. "
            "It does not turn real FBI photos on."
        ),
        "fmr": rate(fmr_count, len(impostor)),
        "fnmr": rate(fnmr_count, len(genuine)),
        "detection_recall_64": rate(det_hit, det_hit + det_miss),
        "below_64_kept": int(measurement["below_64_kept"]),
        "faces_seen_not_compared": int(measurement["faces_seen_not_compared"]),
        "miss_1s": {
            "complete": rate(int(measurement["miss_1s_complete_miss"]), int(measurement["miss_1s_complete_hit"]) + int(measurement["miss_1s_complete_miss"])),
            "measured": rate(int(measurement["miss_1s_measured_miss"]), int(measurement["miss_1s_measured_hit"]) + int(measurement["miss_1s_measured_miss"])),
        },
        "miss_brief": {
            "complete": rate(int(measurement["miss_brief_complete_miss"]), int(measurement["miss_brief_complete_hit"]) + int(measurement["miss_brief_complete_miss"])),
            "measured": rate(int(measurement["miss_brief_measured_miss"]), int(measurement["miss_brief_measured_hit"]) + int(measurement["miss_brief_measured_miss"])),
            "note": "A face visible for about a tenth of a second. The required figure is the one-second rate. This probe shows why measured mode warns that a brief face can be missed.",
        },
        "sweep": sweep(genuine, impostor),
    }


def canonical(value: dict) -> str:
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", required=True)
    parser.add_argument("--bundles", default="bundles")
    parser.add_argument("--out", required=True)
    parser.add_argument("--check", help="Compare a fresh curve to this file")
    args = parser.parse_args()
    proc = subprocess.run(
        [args.bin, "--bundles", args.bundles, "measure"],
        check=False,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        return proc.returncode or 1
    measurement = json.loads(proc.stdout)
    curve = curve_from_measurement(measurement)
    text = canonical(curve)
    if args.check:
        existing = Path(args.check).read_text()
        if existing != text:
            sys.stderr.write("Fast curve does not match this build.\n")
            return 1
    Path(args.out).write_text(text)
    print(f"wrote {args.out}")
    print(
        f"FMR {curve['fmr']['count']}/{curve['fmr']['trials']} "
        f"FNMR {curve['fnmr']['count']}/{curve['fnmr']['trials']} "
        f"recall64 {curve['detection_recall_64']['count']}/{curve['detection_recall_64']['trials']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
