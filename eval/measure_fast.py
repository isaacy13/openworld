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


def score_summary(scores: list[float]) -> dict:
    ordered = sorted(scores)
    mid = ordered[len(ordered) // 2]
    return {"n": len(ordered), "min": ordered[0], "p50": mid, "max": ordered[-1]}


def fail(message: str) -> None:
    raise SystemExit(message)


def separation(genuine: list[float], impostor: list[float], threshold: float) -> dict:
    genuine_min = min(genuine)
    impostor_max = max(impostor)
    overlap = impostor_max >= genuine_min
    if overlap:
        note = "Genuine and impostor scores overlap on this fixture set."
    else:
        note = "The highest impostor score is below the lowest genuine score. The locked cutoff is above some genuine scores, which is the false-negative count."
    return {
        "genuine_min": genuine_min,
        "impostor_max": impostor_max,
        "cutoff": threshold,
        "impostor_max_below_cutoff": threshold - impostor_max,
        "overlap": overlap,
        "note": note,
    }


def check_measurement(measurement: dict, genuine: list[float], impostor: list[float], threshold: float) -> None:
    if measurement.get("perception") != "fiducial-v1":
        fail("Fast curve perception must stay on the fixture markers.")
    true_positive = int(measurement["true_positive"])
    false_negative = int(measurement["false_negative"])
    false_positive = int(measurement["false_positive"])
    true_negative = int(measurement["true_negative"])
    if true_positive + false_negative != len(genuine):
        fail("genuine decisions do not add up to the genuine scores")
    if false_positive + true_negative != len(impostor):
        fail("impostor decisions do not add up to the impostor scores")
    if sum(score >= threshold for score in genuine) != true_positive:
        fail("true positives do not match scores at or above the locked cutoff")
    if sum(score < threshold for score in genuine) != false_negative:
        fail("false negatives do not match scores below the locked cutoff")
    if sum(score >= threshold for score in impostor) != false_positive:
        fail("false positives do not match impostor scores at or above the locked cutoff")
    if sum(score < threshold for score in impostor) != true_negative:
        fail("true negatives do not match impostor scores below the locked cutoff")
    held = [
        "genuine_not_compared",
        "impostor_not_compared",
        "genuine_undetected",
        "impostor_undetected",
        "genuine_wrong_identity",
        "impostor_wrong_identity",
    ]
    for key in held:
        if int(measurement[key]) != 0:
            fail(f"{key} is {measurement[key]}. A comparison trial that did not compare is not published as a true negative.")
    if int(measurement["plate_unpublished_candidate"]) != 0:
        fail("a plate with nothing published became a candidate")
    if int(measurement["below_64_kept"]) != 0:
        fail("a face under 64 px was kept")


def curve_from_measurement(measurement: dict) -> dict:
    genuine = [float(x) for x in measurement["genuine_cosines"]]
    impostor = [float(x) for x in measurement["impostor_cosines"]]
    if not genuine or not impostor:
        fail("measurement has no comparison scores")
    threshold = float(measurement["threshold"])
    check_measurement(measurement, genuine, impostor, threshold)
    fmr_count = sum(score >= threshold for score in impostor)
    fnmr_count = sum(score < threshold for score in genuine)
    det_hit = int(measurement["detection_hit"])
    det_miss = int(measurement["detection_miss"])
    true_positive = int(measurement["true_positive"])
    false_negative = int(measurement["false_negative"])
    false_positive = int(measurement["false_positive"])
    true_negative = int(measurement["true_negative"])
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
            "It does not turn real FBI photos on. "
            "The four-way counts are the locked cutoff on these fixture pairs."
        ),
        "fmr": rate(fmr_count, len(impostor)),
        "fnmr": rate(fnmr_count, len(genuine)),
        "decisions": {
            "definition": (
                "One trial is one fixture marker large enough to compare, against one poster. "
                "True positive: same fixture identity, score at or above the locked cutoff. "
                "False negative: same fixture identity, score below the cutoff. "
                "False positive: different fixture identity, score at or above the cutoff. "
                "True negative: different fixture identity, score below the cutoff. "
                "A face that was not compared is not in this table and is not a clearance."
            ),
            "true_positive": rate(true_positive, len(genuine)),
            "false_negative": rate(false_negative, len(genuine)),
            "false_positive": rate(false_positive, len(impostor)),
            "true_negative": rate(true_negative, len(impostor)),
            "identity_pairs": {
                "genuine": "16 fixture identities, 10 placements each, probe identity equals the poster.",
                "impostor": "160 pairs. Each probe identity is different from the poster and from the other probes.",
            },
        },
        "scores": {
            "genuine": score_summary(genuine),
            "impostor": score_summary(impostor),
            "separation": separation(genuine, impostor, threshold),
        },
        "plates": {
            "match": rate(int(measurement["plate_match_candidate"]), int(measurement["plate_match_trials"])),
            "mismatch_candidate": rate(int(measurement["plate_mismatch_candidate"]), int(measurement["plate_mismatch_trials"])),
            "unpublished_candidate": rate(int(measurement["plate_unpublished_candidate"]), int(measurement["plate_unpublished_trials"])),
            "note": "A match means the crop text equals a plate published on the poster. A mismatch must not become a candidate. An unpublished plate is not read.",
        },
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
        f"TP {curve['decisions']['true_positive']['count']} "
        f"FN {curve['decisions']['false_negative']['count']} "
        f"FP {curve['decisions']['false_positive']['count']} "
        f"TN {curve['decisions']['true_negative']['count']} "
        f"recall64 {curve['detection_recall_64']['count']}/{curve['detection_recall_64']['trials']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
