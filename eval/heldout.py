#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Held-out checks through the shipping scan command.

The published Fast curve is a separate file. These trials use other fixture
identities, other placements, a real compressed video with no marker, a
lossless video of a fixture still that keeps one card per track, a one-frame GIF, a three-frame GIF
whose marker is only on the middle frame, an animated PNG of that marker,
a JPEG with a camera orientation tag, a still WebP with a camera
orientation tag, a PNG with a camera orientation tag, a TIFF with a camera
orientation tag, a 16-bit TIFF with that tag, a palette TIFF, a bilevel TIFF,
a YCbCr TIFF, a subsampled YCbCr TIFF, a two-page TIFF whose marker is only on the second page,
a video whose edit list hides the samples a player does not show,
a video with a quarter-turn
display rotation, a video with non-square pixels, a video whose audio
continues after the pictures, and an audio file.
They are not photographs of people, not LFW, and not a reason to turn real
FBI photos on.
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


def with_orientation(jpeg: bytes, tag: int) -> bytes:
    tiff = bytes([
        0x49, 0x49, 0x2A, 0x00,
        0x08, 0x00, 0x00, 0x00,
        0x01, 0x00,
        0x12, 0x01,
        0x03, 0x00,
        0x01, 0x00, 0x00, 0x00,
        tag & 0xFF, 0x00,
        0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ])
    payload = b"Exif\x00\x00" + tiff
    length = len(payload) + 2
    return bytes([0xFF, 0xD8, 0xFF, 0xE1, (length >> 8) & 0xFF, length & 0xFF]) + payload + jpeg[2:]


def rgb_tiff(pages: list[tuple[int, int, bytes, int]], depth: int = 8) -> bytes:
    """Uncompressed RGB pages. The last value is the camera orientation tag.

    Depth 16 stores each 8-bit sample in the high byte.
    """
    entry_count = 10
    ifd_len = 2 + entry_count * 12 + 4
    cursor = 8
    layout: list[tuple[int, int, int, int]] = []
    for width, height, rgb, _tag in pages:
        if len(rgb) != width * height * 3:
            fail(f"tiff page {width}x{height} has {len(rgb)} bytes")
        stored = len(rgb) * (2 if depth == 16 else 1)
        ifd = cursor
        bits = ifd + ifd_len
        pixels = bits + 6
        cursor = pixels + stored
        layout.append((ifd, bits, pixels, stored))
    out = bytearray(cursor)
    out[0:4] = b"II*\x00"
    out[4:8] = layout[0][0].to_bytes(4, "little")
    for index, (width, height, rgb, tag) in enumerate(pages):
        ifd, bits, pixels, stored = layout[index]
        nxt = layout[index + 1][0] if index + 1 < len(layout) else 0
        out[ifd : ifd + 2] = entry_count.to_bytes(2, "little")
        entries = [
            (256, 4, 1, width),
            (257, 4, 1, height),
            (258, 3, 3, bits),
            (259, 3, 1, 1),
            (262, 3, 1, 2),
            (273, 4, 1, pixels),
            (274, 3, 1, tag),
            (277, 3, 1, 3),
            (278, 4, 1, height),
            (279, 4, 1, stored),
        ]
        at = ifd + 2
        for entry_tag, kind, count, value in entries:
            out[at : at + 2] = entry_tag.to_bytes(2, "little")
            out[at + 2 : at + 4] = kind.to_bytes(2, "little")
            out[at + 4 : at + 8] = count.to_bytes(4, "little")
            out[at + 8 : at + 12] = value.to_bytes(4, "little")
            at += 12
        out[at : at + 4] = nxt.to_bytes(4, "little")
        sample = (16 if depth == 16 else 8).to_bytes(2, "little")
        out[bits : bits + 6] = sample + sample + sample
        if depth == 16:
            wide = bytearray()
            for byte in rgb:
                wide += (byte << 8).to_bytes(2, "little")
            out[pixels : pixels + stored] = wide
        else:
            out[pixels : pixels + stored] = rgb
    return bytes(out)


def png_size(path: Path) -> tuple[int, int]:
    data = path.read_bytes()
    if data[12:16] != b"IHDR":
        fail("png had no header")
    return int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")


def raw_rgb(src: Path, dest: Path, vf: str | None) -> tuple[int, int, bytes]:
    cmd = ["ffmpeg", "-y", "-v", "error", "-i", str(src)]
    if vf:
        cmd += ["-vf", vf]
    cmd += ["-f", "rawvideo", "-pix_fmt", "rgb24", str(dest)]
    proc = run(cmd)
    if proc.returncode != 0 or not dest.is_file():
        fail(proc.stderr or "raw rgb was not written")
    width, height = png_size(src)
    if vf == "transpose=2":
        width, height = height, width
    raw = dest.read_bytes()
    if len(raw) != width * height * 3:
        fail(f"raw rgb length {len(raw)} for {width}x{height}")
    return width, height, raw


def with_png_orientation(data: bytes, tag: int) -> bytes:
    """Insert an eXIf orientation chunk. The stored canvas stays as written."""
    import zlib

    if data[:8] != b"\x89PNG\r\n\x1a\n":
        fail("stored png was not a png")
    tiff = bytes([
        0x49, 0x49, 0x2A, 0x00,
        0x08, 0x00, 0x00, 0x00,
        0x01, 0x00,
        0x12, 0x01,
        0x03, 0x00,
        0x01, 0x00, 0x00, 0x00,
        tag & 0xFF, 0x00,
        0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ])
    typed = b"eXIf" + tiff
    chunk = len(tiff).to_bytes(4, "big") + typed + zlib.crc32(typed).to_bytes(4, "big")
    index = 8
    out = bytearray(data[:8])
    while index + 8 <= len(data):
        length = int.from_bytes(data[index : index + 4], "big")
        kind = data[index + 4 : index + 8]
        block = index + 12 + length
        if block > len(data):
            fail("stored png chunk ran past the file")
        if kind == b"IDAT":
            out += chunk
            out += data[index:]
            return bytes(out)
        out += data[index:block]
        index = block
    fail("stored png had no image data")


def with_webp_orientation(data: bytes, tag: int) -> bytes:
    """Insert a big-endian EXIF orientation chunk. The canvas size stays as stored."""
    if data[:4] != b"RIFF" or data[8:12] != b"WEBP":
        fail("stored webp was not a RIFF WebP")
    chunks: list[tuple[bytes, bytes]] = []
    index = 12
    while index + 8 <= len(data):
        name = data[index : index + 4]
        size = int.from_bytes(data[index + 4 : index + 8], "little")
        start = index + 8
        end = start + size
        if end > len(data):
            fail("stored webp chunk ran past the file")
        chunks.append((name, data[start:end]))
        index = end + (size & 1)
    tiff = bytes([
        0x4D, 0x4D, 0x00, 0x2A,
        0x00, 0x00, 0x00, 0x08,
        0x00, 0x01,
        0x01, 0x12,
        0x00, 0x03,
        0x00, 0x00, 0x00, 0x01,
        0x00, tag & 0xFF,
        0x00, 0x00,
    ])
    width = height = 0
    for name, payload in chunks:
        if name == b"VP8X" and len(payload) >= 10:
            width = int.from_bytes(payload[4:7], "little") + 1
            height = int.from_bytes(payload[7:10], "little") + 1
        elif name == b"VP8L" and len(payload) >= 5 and payload[0] == 0x2F and width == 0:
            bits = int.from_bytes(payload[1:5], "little")
            width = (bits & 0x3FFF) + 1
            height = ((bits >> 14) & 0x3FFF) + 1
    if width <= 0 or height <= 0:
        fail("stored webp did not carry a canvas size")
    vp8x = bytes([
        0x08, 0, 0, 0,
        (width - 1) & 0xFF, ((width - 1) >> 8) & 0xFF, ((width - 1) >> 16) & 0xFF,
        (height - 1) & 0xFF, ((height - 1) >> 8) & 0xFF, ((height - 1) >> 16) & 0xFF,
    ])
    chunks = [(name, payload) for name, payload in chunks if name not in (b"VP8X", b"EXIF")]
    chunks.insert(0, (b"VP8X", vp8x))
    chunks.append((b"EXIF", tiff))
    body = bytearray(b"WEBP")
    for name, payload in chunks:
        body += name
        body += len(payload).to_bytes(4, "little")
        body += payload
        if len(payload) % 2 == 1:
            body.append(0)
    return b"RIFF" + len(body).to_bytes(4, "little") + bytes(body)


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
        "gif_still": 0,
        "gif_later_candidate": 0,
        "apng_later_candidate": 0,
        "oriented_jpeg_candidate": 0,
        "oriented_webp_candidate": 0,
        "oriented_png_candidate": 0,
        "oriented_tiff_candidate": 0,
        "wide_tiff_candidate": 0,
        "palette_tiff_candidate": 0,
        "bilevel_tiff_candidate": 0,
        "ycbcr_tiff_candidate": 0,
        "subsampled_tiff_candidate": 0,
        "tiff_later_candidate": 0,
        "edit_list_candidate": 0,
        "oriented_video_candidate": 0,
        "anamorphic_video_candidate": 0,
        "turned_anamorphic_video_candidate": 0,
        "audio_tail_candidate": 0,
        "audio_refusal": 0,
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
        cards = report.get("candidates") or []
        faces = [card for card in cards if card.get("kind") == "face"]
        plates = [card for card in cards if card.get("kind") == "plate"]
        if len(faces) != 1 or len(plates) != 1:
            fail(f"lossless video cards {[(c.get('kind'), c.get('track_id'), c.get('poster_id'), c.get('frame_index')) for c in cards]}")
        if faces[0].get("poster_id") != "fixture-missing-a" or plates[0].get("poster_id") != "fixture-plate-c":
            fail(f"lossless video posters {faces[0].get('poster_id')} {plates[0].get('poster_id')}")
        passed = [
            row.get("cosine")
            for row in report.get("comparisons") or []
            if row.get("passed") and row.get("poster_id") == faces[0].get("poster_id")
        ]
        if not passed or abs(float(faces[0].get("cosine")) - max(passed)) > 1e-5:
            fail(f"lossless video kept cosine {faces[0].get('cosine')} over {passed}")
        if len(report.get("inventory") or []) <= len(cards):
            fail("lossless video dropped per-frame inventory")
        if len(report.get("comparisons") or []) <= len(faces):
            fail("lossless video dropped per-frame comparisons")
        counts["lossless_video_candidate"] += 1
        counts["timestamp_disagree"] += 1

        gif = root / "blue.gif"
        proc = run(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64",
                "-frames:v",
                "1",
                str(gif),
            ]
        )
        if proc.returncode != 0 or not gif.is_file():
            fail(proc.stderr or "gif was not written")
        report = scan(args.bin, args.bundles, posters, gif, out, "complete")
        assert_clearance(report, "one-frame gif")
        if report.get("frames_decoded") != 1:
            fail(f"one-frame gif decoded {report.get('frames_decoded')} frames")
        counts["gif_still"] += 1

        # The marker is not on the first frame. Keeping only that frame would be a clearance.
        scene = root / "scene.png"
        blank = root / "blank.png"
        wide = root / "wide.png"
        still(args.bin, args.bundles, scene, ["--scene"])
        still(args.bin, args.bundles, blank, ["--blank"])
        header = scene.read_bytes()[:24]
        width = int.from_bytes(header[16:20], "big")
        height = int.from_bytes(header[20:24], "big")
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-i", str(blank),
            "-vf", f"scale={width}:{height}:flags=neighbor", "-frames:v", "1", str(wide),
        ])
        if proc.returncode != 0:
            fail(proc.stderr or "blank frame was not scaled")
        for name, box in (("f0.png", "x=0:y=0"), ("f2.png", "x=20:y=20")):
            proc = run([
                "ffmpeg", "-y", "-v", "error", "-i", str(wide),
                "-vf", f"drawbox={box}:w=1:h=1:color=black:t=fill",
                str(root / name),
            ])
            if proc.returncode != 0:
                fail(proc.stderr or f"{name} was not written")
        shutil.copyfile(scene, root / "f1.png")
        palette = root / "pal.png"
        later = root / "later.gif"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-framerate", "5", "-start_number", "0",
            "-i", str(root / "f%d.png"), "-frames:v", "3",
            "-vf", "palettegen=stats_mode=full:max_colors=8", str(palette),
        ])
        if proc.returncode != 0:
            fail(proc.stderr or "gif palette was not written")
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-framerate", "5", "-start_number", "0",
            "-i", str(root / "f%d.png"), "-i", str(palette), "-frames:v", "3",
            "-lavfi", "paletteuse=dither=none", "-loop", "0", str(later),
        ])
        if proc.returncode != 0 or not later.is_file():
            fail(proc.stderr or "later gif was not written")
        report = scan(args.bin, args.bundles, posters, later, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"later gif summary {report.get('summary')!r} {report.get('message')}")
        if report.get("frames_decoded") != 3:
            fail(f"later gif decoded {report.get('frames_decoded')} frames")
        found = report.get("candidates") or []
        if not found or any(item.get("frame_index") != 1 for item in found):
            fail(f"later gif candidates were not on the middle frame: {found}")
        counts["gif_later_candidate"] += 1

        apng = root / "later.apng"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-framerate", "5", "-start_number", "0",
            "-i", str(root / "f%d.png"), "-frames:v", "3", "-plays", "1", "-f", "apng", str(apng),
        ])
        if proc.returncode != 0 or not apng.is_file():
            fail(proc.stderr or "later apng was not written")
        report = scan(args.bin, args.bundles, posters, apng, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"later apng summary {report.get('summary')!r} {report.get('message')}")
        if report.get("frames_decoded") != 3:
            fail(f"later apng decoded {report.get('frames_decoded')} frames")
        found = report.get("candidates") or []
        if not found or any(item.get("frame_index") != 1 for item in found):
            fail(f"later apng candidates were not on the middle frame: {found}")
        counts["apng_later_candidate"] += 1

        turned = root / "turned.jpg"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-i", str(scene),
            "-vf", "transpose=2", "-q:v", "2", str(turned),
        ])
        if proc.returncode != 0 or not turned.is_file():
            fail(proc.stderr or "turned jpeg was not written")
        side = root / "side.jpg"
        side.write_bytes(with_orientation(turned.read_bytes(), 6))
        report = scan(args.bin, args.bundles, posters, side, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"oriented jpeg summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates"):
            fail("oriented jpeg produced no candidate")
        counts["oriented_jpeg_candidate"] += 1
        report = scan(args.bin, args.bundles, posters, turned, out, "complete")
        assert_clearance(report, "jpeg without the orientation tag")

        stored_webp = root / "stored.webp"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-i", str(scene),
            "-vf", "transpose=2", "-c:v", "libwebp", "-lossless", "1", str(stored_webp),
        ])
        if proc.returncode != 0 or not stored_webp.is_file():
            fail(proc.stderr or "stored webp was not written")
        shown_webp = root / "shown.webp"
        shown_webp.write_bytes(with_webp_orientation(stored_webp.read_bytes(), 6))
        report = scan(args.bin, args.bundles, posters, shown_webp, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"oriented webp summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates"):
            fail("oriented webp produced no candidate")
        counts["oriented_webp_candidate"] += 1
        report = scan(args.bin, args.bundles, posters, stored_webp, out, "complete")
        assert_clearance(report, "webp without the orientation tag")

        stored_png = root / "stored-side.png"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-i", str(scene),
            "-vf", "transpose=2", str(stored_png),
        ])
        if proc.returncode != 0 or not stored_png.is_file():
            fail(proc.stderr or "stored png was not written")
        shown_png = root / "shown-side.png"
        shown_png.write_bytes(with_png_orientation(stored_png.read_bytes(), 6))
        report = scan(args.bin, args.bundles, posters, shown_png, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"oriented png summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates"):
            fail("oriented png produced no candidate")
        counts["oriented_png_candidate"] += 1
        report = scan(args.bin, args.bundles, posters, stored_png, out, "complete")
        assert_clearance(report, "png without the orientation tag")

        side_w, side_h, side_rgb = raw_rgb(scene, root / "side.rgb", "transpose=2")
        shown_tiff = root / "shown.tif"
        shown_tiff.write_bytes(rgb_tiff([(side_w, side_h, side_rgb, 6)]))
        report = scan(args.bin, args.bundles, posters, shown_tiff, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"oriented tiff summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates"):
            fail("oriented tiff produced no candidate")
        counts["oriented_tiff_candidate"] += 1
        wide_tiff = root / "wide.tif"
        wide_tiff.write_bytes(rgb_tiff([(side_w, side_h, side_rgb, 6)], depth=16))
        report = scan(args.bin, args.bundles, posters, wide_tiff, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"16-bit tiff summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates"):
            fail("16-bit tiff produced no candidate")
        counts["wide_tiff_candidate"] += 1
        for (name, pix_fmt, key) in (
            ("palette.tif", "pal8", "palette_tiff_candidate"),
            ("bilevel.tif", "monob", "bilevel_tiff_candidate"),
            ("ycbcr.tif", "yuv444p", "ycbcr_tiff_candidate"),
            ("subsampled.tif", "yuv420p", "subsampled_tiff_candidate"),
        ):
            encoded = root / name
            proc = run([
                "ffmpeg", "-y", "-v", "error", "-i", str(scene),
                "-pix_fmt", pix_fmt, "-compression_algo", "raw", str(encoded),
            ])
            if proc.returncode != 0 or not encoded.is_file():
                fail(proc.stderr or f"{name} was not written")
            report = scan(args.bin, args.bundles, posters, encoded, out, "complete")
            if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
                fail(f"{name} summary {report.get('summary')!r} {report.get('message')}")
            if not report.get("candidates"):
                fail(f"{name} produced no candidate")
            counts[key] += 1
        raw_tiff = root / "raw.tif"
        raw_tiff.write_bytes(rgb_tiff([(side_w, side_h, side_rgb, 1)]))
        report = scan(args.bin, args.bundles, posters, raw_tiff, out, "complete")
        assert_clearance(report, "tiff without the orientation tag")

        scene_w, scene_h, scene_rgb = raw_rgb(scene, root / "scene.rgb", None)
        pages = root / "pages.tif"
        pages.write_bytes(rgb_tiff([
            (2, 2, bytes(12), 1),
            (scene_w, scene_h, scene_rgb, 1),
        ]))
        report = scan(args.bin, args.bundles, posters, pages, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"later tiff summary {report.get('summary')!r} {report.get('message')}")
        if report.get("frames_decoded") != 2:
            fail(f"later tiff decoded {report.get('frames_decoded')} frames")
        found = report.get("candidates") or []
        if not found or any(item.get("frame_index") != 1 for item in found):
            fail(f"later tiff candidates were not on the second page: {found}")
        counts["tiff_later_candidate"] += 1

        width, height = png_size(scene)
        blank_clip = root / "blank-clip.mp4"
        scene_clip = root / "scene-clip.mp4"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-f", "lavfi",
            "-i", f"color=c=black:s={width}x{height}:r=10:d=0.4",
            "-frames:v", "4", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", str(blank_clip),
        ])
        if proc.returncode != 0 or not blank_clip.is_file():
            fail(proc.stderr or "blank clip was not written")
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-loop", "1", "-i", str(scene),
            "-frames:v", "8", "-r", "10", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", str(scene_clip),
        ])
        if proc.returncode != 0 or not scene_clip.is_file():
            fail(proc.stderr or "scene clip was not written")
        hidden_blank = root / "blank-then-scene.mp4"
        hidden_scene = root / "scene-then-blank.mp4"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-i", str(blank_clip), "-i", str(scene_clip),
            "-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0", "-an",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", str(hidden_blank),
        ])
        if proc.returncode != 0 or not hidden_blank.is_file():
            fail(proc.stderr or "blank-then-scene was not written")
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-i", str(scene_clip), "-i", str(blank_clip),
            "-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0", "-an",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", str(hidden_scene),
        ])
        if proc.returncode != 0 or not hidden_scene.is_file():
            fail(proc.stderr or "scene-then-blank was not written")
        shown_edit = root / "edit-scene.mp4"
        blank_edit = root / "edit-blank.mp4"
        proc = run(["ffmpeg", "-y", "-v", "error", "-ss", "0.4", "-i", str(hidden_blank), "-c", "copy", str(shown_edit)])
        if proc.returncode != 0 or not shown_edit.is_file():
            fail(proc.stderr or "edit list was not written")
        proc = run(["ffmpeg", "-y", "-v", "error", "-ss", "0.8", "-i", str(hidden_scene), "-c", "copy", str(blank_edit)])
        if proc.returncode != 0 or not blank_edit.is_file():
            fail(proc.stderr or "blank edit list was not written")
        report = scan(args.bin, args.bundles, posters, shown_edit, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"edit list summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates") or not report.get("frames_decoded"):
            fail("edit list produced no candidate")
        counts["edit_list_candidate"] += 1
        report = scan(args.bin, args.bundles, posters, blank_edit, out, "complete")
        assert_clearance(report, "edit list that hides the marker")

        stored_video = root / "stored.mp4"
        shown_video = root / "shown.mp4"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-loop", "1", "-i", str(scene),
            "-vf", "transpose=2", "-frames:v", "8", "-r", "10", "-an",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", str(stored_video),
        ])
        if proc.returncode != 0 or not stored_video.is_file():
            fail(proc.stderr or "stored video was not written")
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-i", str(stored_video), "-an", "-c:v", "copy",
            "-bsf:v", "h264_metadata=display_orientation=insert:rotate=-90", str(shown_video),
        ])
        if proc.returncode != 0 or not shown_video.is_file():
            fail(proc.stderr or "display rotation was not written")
        report = scan(args.bin, args.bundles, posters, shown_video, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"oriented video summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates") or not report.get("frames_decoded"):
            fail("oriented video produced no candidate")
        counts["oriented_video_candidate"] += 1
        report = scan(args.bin, args.bundles, posters, stored_video, out, "complete")
        assert_clearance(report, "video without the display rotation")

        wide = root / "wide.mp4"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-loop", "1", "-i", str(scene),
            "-vf", "scale=320:480,setsar=2/1", "-frames:v", "4", "-r", "10", "-an",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", str(wide),
        ])
        if proc.returncode != 0 or not wide.is_file():
            fail(proc.stderr or "anamorphic video was not written")
        report = scan(args.bin, args.bundles, posters, wide, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"anamorphic video summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates"):
            fail("anamorphic video produced no candidate")
        counts["anamorphic_video_candidate"] += 1

        turned_wide = root / "turned-wide.mp4"
        turned_stored = root / "turned-stored.mp4"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-loop", "1", "-i", str(scene),
            "-vf", "scale=320:480,setsar=2/1,transpose=2", "-frames:v", "4", "-r", "10", "-an",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", str(turned_stored),
        ])
        if proc.returncode != 0 or not turned_stored.is_file():
            fail(proc.stderr or "turned anamorphic source was not written")
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-i", str(turned_stored), "-an", "-c:v", "copy",
            "-bsf:v", "h264_metadata=display_orientation=insert:rotate=-90", str(turned_wide),
        ])
        if proc.returncode != 0 or not turned_wide.is_file():
            fail(proc.stderr or "turned anamorphic video was not written")
        report = scan(args.bin, args.bundles, posters, turned_wide, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"turned anamorphic summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates"):
            fail("turned anamorphic video produced no candidate")
        counts["turned_anamorphic_video_candidate"] += 1

        pictures = root / "pictures.mkv"
        with_audio = root / "audio-tail.mkv"
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-loop", "1", "-i", str(scene),
            "-frames:v", "8", "-r", "10", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p",
            str(pictures),
        ])
        if proc.returncode != 0 or not pictures.is_file():
            fail(proc.stderr or "picture track was not written")
        proc = run([
            "ffmpeg", "-y", "-v", "error", "-i", str(pictures),
            "-f", "lavfi", "-i", "sine=frequency=440:duration=30",
            "-c:v", "copy", "-c:a", "aac", "-map", "0:v", "-map", "1:a",
            str(with_audio),
        ])
        if proc.returncode != 0 or not with_audio.is_file():
            fail(proc.stderr or "audio tail was not written")
        report = scan(args.bin, args.bundles, posters, with_audio, out, "complete")
        if report.get("status") != "complete" or report.get("summary") != POSSIBLE:
            fail(f"audio tail summary {report.get('summary')!r} {report.get('message')}")
        if not report.get("candidates") or not report.get("frames_decoded"):
            fail("audio tail produced no candidate")
        estimated = run([
            args.bin, "--json", "--bundles", args.bundles, "estimate",
            "--input", str(with_audio), "--bundle", "fast", "--long-side", "640",
            "--coverage", "complete", "--form-factor", "phone", "--provider", "cpu",
        ])
        est = require(estimated, "estimate audio tail")
        decoded = int(report.get("frames_decoded") or 0)
        analyzed = int(est.get("frames_analyzed") or 0)
        if analyzed != decoded or est.get("human") != "Less than a second" or est.get("suggest_computer"):
            fail(f"audio tail estimate {est.get('human')!r} frames {analyzed} decoded {decoded}")
        counts["audio_tail_candidate"] += 1

        tone = root / "tone.wav"
        proc = run(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-c:a",
                "pcm_s16le",
                str(tone),
            ]
        )
        if proc.returncode != 0 or not tone.is_file():
            fail(proc.stderr or "audio was not written")
        report = scan(args.bin, args.bundles, posters, tone, out, "complete")
        if report.get("status") != "refused" or "Refusing." not in (report.get("summary") or ""):
            fail(f"audio summary {report.get('summary')!r}")
        if report.get("candidates") or report.get("frames_decoded"):
            fail("audio was scanned as pictures")
        counts["audio_refusal"] += 1

    print(json.dumps({"ok": True, "counts": counts}, sort_keys=True))
    total = sum(counts.values())
    if (
        counts["impostor_clearance"] != 80
        or counts["gif_still"] != 1
        or counts["gif_later_candidate"] != 1
        or counts["apng_later_candidate"] != 1
        or counts["oriented_jpeg_candidate"] != 1
        or counts["oriented_webp_candidate"] != 1
        or counts["oriented_png_candidate"] != 1
        or counts["oriented_tiff_candidate"] != 1
        or counts["wide_tiff_candidate"] != 1
        or counts["palette_tiff_candidate"] != 1
        or counts["bilevel_tiff_candidate"] != 1
        or counts["ycbcr_tiff_candidate"] != 1
        or counts["subsampled_tiff_candidate"] != 1
        or counts["tiff_later_candidate"] != 1
        or counts["edit_list_candidate"] != 1
        or counts["oriented_video_candidate"] != 1
        or counts["anamorphic_video_candidate"] != 1
        or counts["turned_anamorphic_video_candidate"] != 1
        or counts["audio_tail_candidate"] != 1
        or counts["audio_refusal"] != 1
        or total < 90
    ):
        fail(f"held-out counts are short: {counts}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
