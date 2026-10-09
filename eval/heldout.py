#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Held-out checks through the shipping scan command.

The published Fast curve is a separate file. These trials use other fixture
identities, other placements, a real compressed video with no marker, and a
lossless video of a fixture still. They are not photographs of people, not
LFW, and not a reason to turn real FBI photos on.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

POSSIBLE = "Possible candidate. Not an identification."
CLEARANCE = "No candidate is not a clearance."
NOT_COMPARED = "Not compared."
INCOMPLETE = "Incomplete."
VEHICLE = "A vehicle is not a person."
BELOW = "Below the locked cutoff. Not a candidate."
DISAGREE = "The file timestamps disagree."
BRIEF = "A brief face can be missed."
BANNED = ("Identified", "more accurate", "found this person")


def fail(message: str) -> None:
    raise SystemExit(message)


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, check=False, capture_output=True, text=True)


def require(proc: subprocess.CompletedProcess[str], what: str) -> dict:
    if not proc.stdout.strip():
        fail(f"{what} returned no JSON\n{proc.stderr}")
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        fail(f"{what} was not JSON: {exc}\n{proc.stdout[:400]}")


def scan(bin_path: str, bundles: str, posters: Path, image: Path, out: Path, coverage: str) -> dict:
    proc = run(
        [
            bin_path,
            "--json",
            "--bundles",
            bundles,
            "scan",
            "--input",
            str(image),
            "--bundle",
            "fast",
            "--long-side",
            "640",
            "--coverage",
            coverage,
            "--posters",
            str(posters),
            "--out",
            str(out),
            "--form-factor",
            "computer",
            "--provider",
            "cpu",
        ]
    )
    report = require(proc, f"scan {image.name}")
    blob = json.dumps(report)
    for phrase in BANNED:
        if phrase in blob:
            fail(f"{image.name} contains {phrase!r}")
    return report


def still(bin_path: str, bundles: str, dest: Path, args: list[str]) -> None:
    proc = run([bin_path, "--json", "--bundles", bundles, "fixture-still", "--out", str(dest), *args])
    require(proc, "fixture-still")
    if proc.returncode != 0:
        fail(proc.stderr or proc.stdout)


def assert_clearance(report: dict, label: str) -> None:
    if report.get("status") != "complete":
        fail(f"{label} status {report.get('status')} {report.get('message')}")
    if report.get("summary") != CLEARANCE:
        fail(f"{label} summary {report.get('summary')!r}")
    if report.get("candidates"):
        fail(f"{label} became a candidate")
    if report.get("summary") == INCOMPLETE:
        fail(f"{label} was marked incomplete")


def assert_not_candidate_face(report: dict, label: str, expected_label: str | None) -> None:
    assert_clearance(report, label)
    labels = [item.get("label") for item in report.get("inventory") or []]
    if expected_label is not None and expected_label not in labels:
        fail(f"{label} inventory {labels}")
    if POSSIBLE in labels:
        fail(f"{label} labeled a possible candidate")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", required=True)
    parser.add_argument("--bundles", default="bundles")
    args = parser.parse_args()
    if not Path(args.bin).is_file():
        fail(f"missing binary {args.bin}")
    if shutil.which("ffmpeg") is None or shutil.which("ffprobe") is None:
        fail("ffmpeg and ffprobe are required for the video sets")

    counts = {
        "impostor_clearance": 0,
        "below_cutoff": 0,
        "under_64": 0,
        "seen_not_compared": 0,
        "scene_candidate": 0,
        "blank_clearance": 0,
        "compressed_video_clearance": 0,
        "lossless_video_candidate": 0,
        "timestamp_disagree": 0,
        "measured_banner": 0,
    }
    with tempfile.TemporaryDirectory(prefix="openworld-heldout-") as tmp:
        root = Path(tmp)
        posters = root / "posters"
        proc = run([args.bin, "--json", "--bundles", args.bundles, "posters", "write-fixture", "--out", str(posters)])
        require(proc, "write-fixture")
        if proc.returncode != 0:
            fail("fixture pack was not written")
        image = root / "frame.png"
        out = root / "result"

        # Identities other than the two fixture posters (7 and 11), at placements
        # the published curve does not use. None of these may pass the cutoff.
        impostor_ids = [1, 2, 3, 4, 5, 6, 8, 9, 10, 12, 13, 14, 15, 16, 20, 40, 80, 99, 200, 400]
        places = [(16, 16), (48, 16), (80, 24), (120, 40)]
        for fid in impostor_ids:
            for x, y in places:
                still(
                    args.bin,
                    args.bundles,
                    image,
                    ["--id", str(fid), "--module", "16", "--x", str(x), "--y", str(y)],
                )
                report = scan(args.bin, args.bundles, posters, image, out, "complete")
                assert_clearance(report, f"impostor {fid} at {x},{y}")
                for row in report.get("comparisons") or []:
                    if row.get("passed"):
                        fail(f"impostor {fid} passed the locked cutoff")
                counts["impostor_clearance"] += 1

        for fid in (7, 11):
            still(args.bin, args.bundles, image, ["--id", str(fid), "--module", "16", "--below-cutoff"])
            report = scan(args.bin, args.bundles, posters, image, out, "complete")
            assert_not_candidate_face(report, f"below {fid}", BELOW)
            counts["below_cutoff"] += 1

        still(args.bin, args.bundles, image, ["--id", "7", "--module", "4", "--x", "16", "--y", "16"])
        report = scan(args.bin, args.bundles, posters, image, out, "complete")
        assert_clearance(report, "under 64")
        if report.get("inventory"):
            fail("a face under 64 px was kept")
        if report.get("faces_seen_not_compared"):
            fail("a face under 64 px was counted as seen")
        counts["under_64"] += 1

        still(args.bin, args.bundles, image, ["--id", "11", "--module", "8", "--x", "16", "--y", "16"])
        report = scan(args.bin, args.bundles, posters, image, out, "complete")
        assert_not_candidate_face(report, "seen not compared", NOT_COMPARED)
        if not report.get("faces_seen_not_compared"):
            fail("a 64 px face was not recorded as seen")
        counts["seen_not_compared"] += 1

        still(args.bin, args.bundles, image, ["--scene"])
        report = scan(args.bin, args.bundles, posters, image, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"scene summary {report.get('summary')!r}")
        labels = [item.get("label") for item in report.get("inventory") or []]
        for phrase in (POSSIBLE, NOT_COMPARED, VEHICLE):
            if phrase not in labels:
                fail(f"scene inventory missing {phrase}: {labels}")
        if not report.get("candidates"):
            fail("scene produced no candidate")
        counts["scene_candidate"] += 1

        still(args.bin, args.bundles, image, ["--blank"])
        report = scan(args.bin, args.bundles, posters, image, out, "complete")
        assert_clearance(report, "blank")
        if report.get("inventory"):
            fail("a blank still produced inventory")
        counts["blank_clearance"] += 1

        bars = root / "bars.mp4"
        proc = run(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=10:duration=1",
                "-an",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                str(bars),
            ]
        )
        if proc.returncode != 0 or not bars.is_file():
            fail(proc.stderr or "testsrc2 video was not written")
        report = scan(args.bin, args.bundles, posters, bars, out, "complete")
        assert_clearance(report, "compressed video")
        if not report.get("frames_decoded"):
            fail("compressed video decoded no frames")
        note = report.get("perception_note") or ""
        if "Fixture markers were read." not in note:
            fail(f"compressed video perception note {note!r}")
        counts["compressed_video_clearance"] += 1

        report = scan(args.bin, args.bundles, posters, bars, out, "measured")
        if report.get("status") != "complete":
            fail(f"measured video status {report.get('status')}")
        if report.get("coverage_banner") != BRIEF:
            fail(f"measured video banner {report.get('coverage_banner')!r}")
        if report.get("summary") == INCOMPLETE:
            fail("measured video was marked incomplete")
        counts["measured_banner"] += 1

        still(args.bin, args.bundles, image, ["--scene"])
        movie = root / "scene.mkv"
        proc = run(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-loop",
                "1",
                "-i",
                str(image),
                "-frames:v",
                "8",
                "-r",
                "10",
                "-an",
                "-c:v",
                "ffv1",
                "-pix_fmt",
                "rgb24",
                "-metadata",
                "creation_time=2020-01-01T00:00:00Z",
                str(movie),
            ]
        )
        if proc.returncode != 0 or not movie.is_file():
            fail(proc.stderr or "lossless video was not written")
        report = scan(args.bin, args.bundles, posters, movie, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"lossless video summary {report.get('summary')!r} {report.get('message')}")
        if DISAGREE not in (report.get("warnings") or []):
            fail(f"lossless video warnings {report.get('warnings')}")
        if not report.get("candidates"):
            fail("lossless video of the fixture scene produced no candidate")
        counts["lossless_video_candidate"] += 1
        counts["timestamp_disagree"] += 1

    print(json.dumps({"ok": True, "counts": counts}, sort_keys=True))
    total = sum(counts.values())
    if counts["impostor_clearance"] != 80 or total < 88:
        fail(f"held-out counts are short: {counts}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
