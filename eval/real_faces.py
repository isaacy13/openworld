#!/usr/bin/env python3
"""Measure a network bundle on public face photographs, and write its curve.

The photographs are downloaded into --data and checked against pinned SHA-256
digests. None of them are committed. Only counts, rates, intervals, and score
thresholds are written to --out.

What runs where:

- `openworld-eval vectors` (eval/real) prints every face the scan would see in
  a photo, what the quality gates said, and the vector it would compare. It
  calls the library's `perception::face_vectors`: the same decode, detection
  size, gates, alignment, and embedder as a scan, on the CPU.
- `openworld scan` (built with `--features eval`, so each track carries its
  poster scores) scans stills and H.264 clips against a poster pack that
  `openworld-eval pack` built from other LFW photos. The scores the scan
  reports must equal the library's cosines.

LFW (Huang et al., 2007) sets the cutoff: every same-person and every
different-person pair, so a false match rate near one in a million has trials
behind it. CALFW and CPLFW are held out: the cutoff chosen on LFW is applied to
their pairs unchanged. WIDER FACE validation (Yang et al., 2016) measures
detection by face size on the original image.

LFW faces are about 94 px, under the 112 px compare rule. They are enlarged
twice with a bicubic filter. The embedder still reads a 112 px aligned crop,
so this measures faces about that size, not larger ones.

The labeled person in an LFW photo is the detected face nearest the middle. If
that face did not pass the gates, the photo has no compared face: another face
in the photo is someone else, and is never used in its place.
"""

import argparse
import csv
import hashlib
import json
import math
import os
import platform
import random
import re
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib
import urllib.error
import urllib.request
import zipfile
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stats  # noqa: E402
from stats import rate  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
# Mirrors are tried in order. Each download must still match its pin below.
SOURCES = {
    "lfw.tgz": (
        "https://ndownloader.figshare.com/files/5976018",
        "https://figshare.com/ndownloader/files/5976018",
        "https://api.figshare.com/v2/file/download/5976018",
    ),
    "pairs.txt": (
        "https://ndownloader.figshare.com/files/5976006",
        "https://figshare.com/ndownloader/files/5976006",
        "https://api.figshare.com/v2/file/download/5976006",
        "https://raw.githubusercontent.com/davidsandberg/facenet/096ed770f163957c1e56efa7feeb194773920f6e/data/pairs.txt",
    ),
    "WIDER_val.zip": ("https://huggingface.co/datasets/CUHK-CSE/wider_face/resolve/main/data/WIDER_val.zip",),
    "wider_face_split.zip": ("https://huggingface.co/datasets/CUHK-CSE/wider_face/resolve/main/data/wider_face_split.zip",),
}
USER_AGENT = "openworld-eval (+https://github.com/isaacy13/openworld)"
# A published curve is only comparable to a run on the same bytes.
PINS = {
    "lfw.tgz": "055f7d9c632d7370e6fb4afc7468d40f970c34a80d4c6f50ffec63f5a8d536c0",
    "pairs.txt": "ea42330c62c92989f9d7c03237ed5d591365e89b3e649747777b70e692dc1592",
    "WIDER_val.zip": "f9efbd09f28c5d2d884be8c0eaef3967158c866a593fc36ab0413e4b2a58a17a",
    "wider_face_split.zip": "c7561e4f5e7a118c249e0a5c5c902b0de90bbf120d7da9fa28d99041f68a8a5c",
}
# Held out from the cutoff: cross-age LFW (Zheng, Deng, Hu 2017) and cross-pose
# LFW (Zheng, Deng 2018), from a Hugging Face mirror read at one commit. Each
# has 6,000 pairs in ten folds. Only the photos those pairs name are downloaded.
HELD_OUT = {
    "calfw": {
        "repo": "marcelohaps/calfw",
        "commit": "4f3e4cda32f6bdd163a46c42c6cc313629397700",
        "folder": "raw",
        "columns": ("image_a_raw_path", "image_b_raw_path"),
        "what": "Cross-age LFW: the two photos of a same-person pair are years apart.",
    },
    "cplfw": {
        "repo": "marcelohaps/cplfw",
        "commit": "89b5236a99194f93382774d70b4bbcb18afa77ec",
        "folder": "train",
        "columns": ("image_a_path", "image_b_path"),
        "what": "Cross-pose LFW: one photo of a same-person pair is often turned toward profile.",
    },
}
# SHA-256 over pairs.csv and every photo it names, in path order, after a full download.
HELD_OUT_PINS = {
    "calfw": "659b22911774ca45f2e7a4f0ebc4a76e3deac603172ea7b9f6dfb7e6825db56d",
    "cplfw": "93681bda53ca934cca461ac2120835c40672dfea46c176d86c1894d61434fe57",
}

CURVE_SCHEMA = "openworld.curve.v1"
PICKER_LINE = "LFW curve measured. Real FBI photos stay off."
# A scan compares every face in a file with every enabled poster, so the cutoff
# is set where one comparison in a million is a false match.
CUTOFF_FMR = 1e-6
FMR_TARGETS = [1e-3, 1e-4, 1e-5, 1e-6]
# The operating point a sample run must reproduce, and how closely.
CHECK_FMR = 1e-4
CHECK_SCORE_TOLERANCE = 0.01
# The scan and the library must agree to this many units of cosine.
COSINE_TOLERANCE = 1e-4
# The FMR / FNMR table, every 0.02.
CURVE_THRESHOLDS = [round(0.20 + 0.02 * i, 2) for i in range(36)]
HISTOGRAM_BINS = 20000
BOOTSTRAP_REPLICATES = 1000
BOOTSTRAP_SEED = 20261019
ENLARGE = 2
CHANCE_SOURCE = "LFW all pairs, faces enlarged 2x"
COMPARED = ("candidate", "below_cutoff", "too_brief")

# Clips: 30 frames a second, a gray screen, then the face for one second while
# it drifts and grows a little, then the gray screen again.
FPS = 30
LEAD_FRAMES = 15
ON_FRAMES = 30
TAIL_FRAMES = 15
BRIEF_FRAMES = 2
BACKGROUND = (0x6B, 0x72, 0x80)


# Downloads and pins.


def sha256_file(path: Path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while block := handle.read(1 << 20):
            digest.update(block)
    return digest.hexdigest()


def fetch(data: Path, need):
    data.mkdir(parents=True, exist_ok=True)
    for name in need:
        target = data / name
        for url in SOURCES[name]:
            if not target.exists():
                print(f"downloading {name} from {url}", file=sys.stderr)
                try:
                    download(url, target)
                except OSError as error:
                    print(f"{url}: {error}", file=sys.stderr)
                    continue
            if sha256_file(target) == PINS[name]:
                break
            target.unlink()
            print(f"{url}: not the pinned file. Deleted it.", file=sys.stderr)
        else:
            raise SystemExit(f"No mirror gave the pinned {name}. Refusing.")
    if "lfw.tgz" in need and not (data / "lfw").is_dir():
        with tarfile.open(data / "lfw.tgz") as archive:
            archive.extractall(data, filter="data")
    if "WIDER_val.zip" in need and not (data / "WIDER_val").is_dir():
        zipfile.ZipFile(data / "WIDER_val.zip").extractall(data)
    if "wider_face_split.zip" in need and not (data / "wider_face_split").is_dir():
        zipfile.ZipFile(data / "wider_face_split.zip").extractall(data)


def download(url: str, target: Path, attempts=6, rate_limit_waits=30):
    target.parent.mkdir(parents=True, exist_ok=True)
    part = target.with_suffix(target.suffix + ".part")
    failures = waits = 0
    while True:
        try:
            # Some hosts refuse Python's default User-Agent.
            request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
            with urllib.request.urlopen(request, timeout=120) as response, part.open("wb") as out:
                while block := response.read(1 << 20):
                    out.write(block)
            part.replace(target)
            return
        except OSError as error:
            # A rate limit is waited out; it does not count as a failure.
            if isinstance(error, urllib.error.HTTPError) and error.code == 429 and waits < rate_limit_waits:
                waits += 1
                retry_after = error.headers.get("Retry-After", "")
                time.sleep(int(retry_after) if retry_after.isdigit() else 60)
                continue
            failures += 1
            if failures == attempts:
                raise
            time.sleep(2**failures)


def held_out_pairs(root: Path, spec, folds=None):
    with (root / "pairs.csv").open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    a_col, b_col = spec["columns"]
    out = []
    for row in rows:
        fold = int(row["fold_id"])
        if folds is None or fold in folds:
            out.append((fold - 1, row["is_same"] == "1", root / spec["folder"] / row[a_col], root / spec["folder"] / row[b_col]))
    return out


def held_out_digest(root: Path, spec):
    """The pinned digest: pairs.csv and every photo it names, in path order."""
    pairs = held_out_pairs(root, spec)
    photos = sorted({p for _, _, a, b in pairs for p in (a, b)})
    if not all(p.is_file() for p in photos):
        return None
    digest = hashlib.sha256()
    for path in [root / "pairs.csv", *photos]:
        digest.update(path.relative_to(root).as_posix().encode())
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def fetch_held_out(data: Path, name, folds=None):
    """The pairs, downloaded as needed. Verified when every photo of the set is on disk."""
    spec = HELD_OUT[name]
    root = data / name
    base = f"https://huggingface.co/datasets/{spec['repo']}/resolve/{spec['commit']}"
    if not (root / "pairs.csv").is_file():
        download(f"{base}/pairs.csv", root / "pairs.csv")
    pairs = held_out_pairs(root, spec, folds)
    photos = sorted({p for _, _, a, b in pairs for p in (a, b)})
    missing = [p for p in photos if not p.is_file()]
    if missing:
        print(f"downloading {len(missing)} {name} photos", file=sys.stderr)
        # Few at a time: the mirror rate-limits anonymous downloads.
        with ThreadPoolExecutor(4) as pool:
            list(pool.map(lambda p: download(f"{base}/{spec['folder']}/{p.relative_to(root / spec['folder']).as_posix()}", p), missing))
    got = held_out_digest(root, spec)
    if got is None:
        if folds is None:
            raise SystemExit(f"{name} is missing photos after the download. Refusing.")
        print(f"{name}: only the sampled photos are on disk, so the set digest is not checked", file=sys.stderr)
        return pairs, False
    if got != HELD_OUT_PINS[name]:
        raise SystemExit(f"{name} is not the pinned set ({got}). Refusing.")
    return pairs, True


# The library's view of a photo.


class Vectors:
    """Runs `openworld-eval vectors` in parallel shards and caches the rows."""

    def __init__(self, eval_bin, bundles, bundle, cache: Path, jobs):
        self.eval_bin, self.bundles, self.bundle, self.cache, self.jobs = eval_bin, bundles, bundle, cache, jobs
        manifest = (Path(bundles) / bundle / "manifest.toml").read_text()
        # The vectors depend on the weights and the tool, not on the threshold.
        weights = "".join(l for l in manifest.splitlines() if l.startswith("sha256"))
        tool = sha256_file(Path(eval_bin))
        self.key = hashlib.sha256((weights + tool).encode()).hexdigest()[:16]
        self.providers = set()
        cache.mkdir(parents=True, exist_ok=True)

    def rows(self, paths, enlarge=1):
        tag = hashlib.sha256(("\n".join(map(str, paths)) + f"|{enlarge}|" + self.key).encode()).hexdigest()[:16]
        cached = self.cache / f"vectors-{tag}.jsonl"
        if cached.is_file():
            lines = cached.read_text().splitlines()
        else:
            shards = [paths[i :: self.jobs] for i in range(self.jobs)]
            with ThreadPoolExecutor(self.jobs) as pool:
                lines = [line for part in pool.map(lambda s: self.run(s, enlarge), shards) for line in part]
            cached.write_text("\n".join(lines) + "\n")
        rows = {row["path"]: row for row in map(json.loads, lines)}
        self.providers.update(row["provider"] for row in rows.values() if "provider" in row)
        missing = [p for p in map(str, paths) if p not in rows]
        if missing:
            raise SystemExit(f"openworld-eval returned no row for {len(missing)} photos, e.g. {missing[0]}")
        return rows

    def run(self, shard, enlarge):
        if not shard:
            return []
        with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as handle:
            handle.write("\n".join(map(str, shard)))
        try:
            done = subprocess.run(
                [self.eval_bin, "vectors", "--bundles", self.bundles, "--bundle", self.bundle,
                 "--list", handle.name, "--enlarge", str(enlarge)],
                capture_output=True, text=True,
            )
        finally:
            os.unlink(handle.name)
        if done.returncode != 0:
            raise SystemExit(f"openworld-eval vectors failed: {done.stderr.strip()}")
        return done.stdout.splitlines()


def center_face(row):
    """The detected face nearest the middle of the photo, compared or not."""
    faces = row.get("faces") or []
    if not faces:
        return None
    cx, cy = row["width"] / 2, row["height"] / 2

    def distance(f):
        x, y, w, h = f["rect"]
        return (x + w / 2 - cx) ** 2 + (y + h / 2 - cy) ** 2

    return min(faces, key=distance)


def is_compared(face):
    return face is not None and face.get("embedding") is not None


def outcome(face):
    """`compared`, a quality-gate code such as `why_pose`, or `no_face`."""
    if face is None:
        return "no_face"
    return "compared" if face.get("embedding") is not None else (face.get("reject") or "not_compared")


def gate_summary(rows, paths):
    """How many photos have a compared face, and why the others do not."""
    paths = [str(p) for p in paths]
    decoded = [p for p in paths if "error" not in rows[p]]
    center = Counter(outcome(center_face(rows[p])) for p in decoded)
    every = Counter(outcome(f) for p in decoded for f in rows[p].get("faces", []))
    return {
        "photos": len(paths),
        "not_decoded": len(paths) - len(decoded),
        "face_seen": rate(sum(1 for p in decoded if rows[p].get("faces")), len(paths)),
        "center_face_compared": rate(center["compared"], len(paths)),
        "center_face": dict(sorted(center.items())),
        "every_face": dict(sorted(every.items())),
    }


# LFW all pairs.


def lfw_images(data: Path, limit_people=None):
    people = sorted(p for p in (data / "lfw").iterdir() if p.is_dir())
    if limit_people:
        people = people[:: max(1, len(people) // limit_people)][:limit_people]
    images = []
    for person in people:
        for image in sorted(person.glob("*.jpg")):
            images.append((person.name, image))
    return images


def lfw_pairs(data: Path):
    lines = (data / "pairs.txt").read_text().split("\n")
    folds, size = map(int, lines[0].split())
    rows = [line.split() for line in lines[1:] if line.strip()]
    pairs = []
    for fold in range(folds):
        block = rows[fold * 2 * size : (fold + 1) * 2 * size]
        for parts in block[:size]:
            pairs.append((fold, True, (parts[0], int(parts[1])), (parts[0], int(parts[2]))))
        for parts in block[size:]:
            pairs.append((fold, False, (parts[0], int(parts[1])), (parts[2], int(parts[3]))))
    return pairs


def lfw_path(data, name, index):
    return data / "lfw" / name / f"{name}_{index:04d}.jpg"


def ten_fold(scores, same, folds):
    """Accuracy with the threshold chosen on the other nine folds, as LFW reports it."""
    accuracies = []
    candidates = np.unique(np.round(scores, 4))
    for fold in range(10):
        train = folds != fold
        best_t, best_acc = 0.0, -1.0
        for t in candidates:
            acc = np.mean((scores[train] >= t) == same[train])
            if acc > best_acc:
                best_t, best_acc = t, acc
        test = folds == fold
        accuracies.append(float(np.mean((scores[test] >= best_t) == same[test])))
    return float(np.mean(accuracies)), float(np.std(accuracies))


def scan_pairs(embeddings, groups, floor=None, block=1024):
    """Every pair i < j, a block of rows at a time.

    Without `floor`: a histogram of different-person scores and every
    same-person pair. With `floor`: every different-person pair at or above it."""
    n = len(embeddings)
    hist = np.zeros(HISTOGRAM_BINS, dtype=np.int64)
    found = ([], [], [])
    cols = np.arange(n)[None, :]
    for start in range(0, n, block):
        stop = min(n, start + block)
        part = embeddings[start:stop] @ embeddings.T
        upper = cols > np.arange(start, stop)[:, None]
        same = groups[start:stop, None] == groups[None, :]
        if floor is None:
            hist += np.bincount(stats.histogram_index(part[upper & ~same], HISTOGRAM_BINS), minlength=HISTOGRAM_BINS)
            rows, cols_hit = np.nonzero(upper & same)
        else:
            rows, cols_hit = np.nonzero(upper & ~same & (part >= np.float32(floor)))
        found[0].append(rows + start)
        found[1].append(cols_hit)
        found[2].append(part[rows, cols_hit])
    i, j, s = (np.concatenate(x) if x else np.zeros(0) for x in found)
    return hist, i.astype(np.int64), j.astype(np.int64), s.astype(np.float32)


def histogram_floor(hist, total, target):
    """A bin edge low enough that the scores above it reach past the cutoff for `target`."""
    allowed = math.floor(target * total)
    above = np.cumsum(hist[::-1])[::-1]
    index = int(np.nonzero(above > allowed)[0].max()) if (above > allowed).any() else 0
    return -1.0 + max(0, index - 1) * 2.0 / len(hist)


def lfw_statistics(embeddings, names, threshold, extra_thresholds=(), replicates=BOOTSTRAP_REPLICATES, seed=BOOTSTRAP_SEED):
    """FMR and FNMR over every pair of compared photos, with Wilson and identity-bootstrap intervals.

    `names` is each photo's identity, in photo order (a person's photos in a row)."""
    embeddings = np.ascontiguousarray(embeddings, dtype=np.float32)
    labels, groups = np.unique(np.asarray(names), return_inverse=True)
    groups = groups.astype(np.int64)
    images_per_group = np.bincount(groups, minlength=len(labels))
    hist, gen_i, gen_j, gen_s = scan_pairs(embeddings, groups)
    gen_g = groups[gen_i]
    total = int(hist.sum())
    wanted = [threshold, *extra_thresholds]
    floor = min([histogram_floor(hist, total, max(FMR_TARGETS)), *wanted]) - 2.0 / HISTOGRAM_BINS
    _, tail_i, tail_j, tail_s = scan_pairs(embeddings, groups, floor)
    order = np.argsort(-tail_s, kind="stable")
    tail_i, tail_j, tail_s = tail_i[order], tail_j[order], tail_s[order]
    tail_ga, tail_gb = groups[tail_i], groups[tail_j]

    # One pair per person: that person's first two compared photos.
    firsts = {}
    for index, g in enumerate(groups):
        firsts.setdefault(int(g), []).append(index)
    pp = [(g, idx[0], idx[1]) for g, idx in sorted(firsts.items()) if len(idx) >= 2]
    pp_g = np.array([g for g, _, _ in pp], dtype=np.int64)
    pp_s = np.array([float(embeddings[a] @ embeddings[b]) for _, a, b in pp], dtype=np.float32)

    weights = stats.bootstrap_weights(len(labels), replicates, seed)
    impostor_totals = stats.impostor_totals(weights, images_per_group)
    genuine_per_group = weights @ np.bincount(gen_g, minlength=len(labels))
    pp_per_group = weights @ np.bincount(pp_g, minlength=len(labels))

    def point(t):
        t = round(float(t), 4)
        if t < floor:
            raise ValueError(f"threshold {t} is under the collected tail {floor}")
        hit = tail_s >= np.float32(t)
        a, b, counts, _ = stats.aggregate_pairs(tail_ga[hit], tail_gb[hit], np.ones(int(hit.sum())))
        fm_boot = stats.bootstrap_rate(stats.pair_sums(weights, a, b, counts), impostor_totals)
        miss = gen_s < np.float32(t)
        fnm_boot = stats.bootstrap_rate(weights @ np.bincount(gen_g[miss], minlength=len(labels)), genuine_per_group)
        pp_miss = pp_s < np.float32(t)
        pp_boot = stats.bootstrap_rate(weights @ np.bincount(pp_g[pp_miss], minlength=len(labels)), pp_per_group)
        return {
            "threshold": t,
            "false_match": rate(int(hit.sum()), total, fm_boot),
            "false_match_spread": stats.false_match_spread(tail_i[hit], tail_j[hit], tail_ga[hit], tail_gb[hit]),
            "false_non_match": rate(int(miss.sum()), len(gen_s), fnm_boot),
            "false_non_match_one_pair_per_person": rate(int(pp_miss.sum()), len(pp_s), pp_boot),
        }

    operating = []
    for target in FMR_TARGETS:
        t = stats.cutoff_from_tail(tail_s, total, target)
        operating.append({"target_fmr": target, **point(t)} if t is not None else
                         {"target_fmr": target, "threshold": None, "note": "Too few different-person pairs to reach this rate."})
    cutoff = stats.cutoff_from_tail(tail_s, total, CUTOFF_FMR)
    cutoffs = stats.bootstrap_cutoffs(weights, tail_ga, tail_gb, tail_s, impostor_totals, CUTOFF_FMR)
    table = []
    for t in CURVE_THRESHOLDS:
        k = int(round((t + 1.0) * HISTOGRAM_BINS / 2.0))
        table.append({
            "threshold": t,
            "false_match": rate(int(hist[k:].sum()), total),
            "false_non_match": rate(stats.count_below(gen_s, t), len(gen_s)),
        })
    section = {
        "photos_compared": int(len(embeddings)),
        "identities_compared": int(len(labels)),
        "genuine_pairs": int(len(gen_s)),
        "impostor_pairs": total,
        "people_with_two_compared_photos": int(len(pp_s)),
        "operating_points": operating,
        "at_manifest_threshold": point(threshold),
        "at_measured_cutoff": point(cutoff) if cutoff is not None else None,
        "measured_cutoff_identity_bootstrap95": stats.percentile95(cutoffs),
        "at_thresholds": {f"{t:.4f}": point(t) for t in sorted(set(round(float(x), 4) for x in extra_thresholds))},
        "curve": table,
        "curve_note": "False matches in the table are counted in 0.0001 score bins; the operating points are exact.",
        "intervals": (
            f"wilson95 treats pairs as independent. identity_bootstrap95 resamples the {len(labels)} people "
            f"{replicates} times (seed {seed}), so a person's pairs move together."
        ),
    }
    return section, cutoff


def measure_lfw(args, vectors, data, threshold, extra_thresholds=()):
    images = lfw_images(data, args.people)
    paths = [str(p) for _, p in images]
    big = vectors.rows(paths, enlarge=ENLARGE)
    native = vectors.rows(paths, enlarge=1)
    chosen = {p: center_face(big[p]) for p in paths}
    found = [p for p in paths if is_compared(chosen[p])]
    if len(found) < 2:
        raise SystemExit("fewer than two LFW photos have a compared face")
    embeddings = np.array([chosen[p]["embedding"] for p in found], dtype=np.float32)
    names = [Path(p).parent.name for p in found]
    section, cutoff = lfw_statistics(embeddings, names, threshold, extra_thresholds)
    section = {
        "images": len(paths),
        "identities": len({Path(p).parent.name for p in paths}),
        "faces": "enlarged 2x; the center face of each photo",
        "center_face_compared": rate(len(found), len(paths)),
        **section,
    }
    section["miss_rate_headline"] = {
        "what": "One poster against one photo: each person's first two compared photos, at the manifest threshold.",
        **section["at_manifest_threshold"]["false_non_match_one_pair_per_person"],
    }
    if args.people is None:
        scores, same, folds, missing = [], [], [], 0
        for fold, match, a, b in lfw_pairs(data):
            fa, fb = chosen.get(str(lfw_path(data, *a))), chosen.get(str(lfw_path(data, *b)))
            if not (is_compared(fa) and is_compared(fb)):
                missing += 1
                score = -1.0
            else:
                score = float(np.dot(fa["embedding"], fb["embedding"]))
            scores.append(score)
            same.append(match)
            folds.append(fold)
        mean, std = ten_fold(np.array(scores), np.array(same), np.array(folds))
        section["ten_fold"] = {"pairs": len(scores), "pairs_without_a_compared_face": missing,
                               "accuracy_mean": round(mean, 4), "accuracy_std": round(std, 4)}
    quality = {"lfw_enlarged_2x": gate_summary(big, paths), "lfw_native_size": gate_summary(native, paths)}
    return section, quality, cutoff


# Held-out pairs.


def identity_of(path):
    """LFW-style names: `First_Last_0003.jpg` is First_Last. A bare number is its folder's person."""
    stem = Path(path).stem
    match = re.match(r"^(.*?)[_-]?\d+$", stem)
    if match and match.group(1):
        return match.group(1)
    return Path(path).parent.name if match else stem


def pair_set_statistics(pairs, threshold, replicates=BOOTSTRAP_REPLICATES, seed=BOOTSTRAP_SEED):
    """`pairs` is (same person, score or None, identity a, identity b).

    A same-person pair without a compared face on both photos is a miss: the
    scan would not card it. A different-person pair without one is not a true
    negative; it is counted apart, as not compared."""
    labels = sorted({x for _, _, a, b in pairs for x in (a, b)})
    index = {name: i for i, name in enumerate(labels)}
    weights = stats.bootstrap_weights(len(labels), replicates, seed)
    t = np.float32(threshold)
    same = [(index[a], s) for match, s, a, _ in pairs if match]
    diff = [(index[a], index[b], s) for match, s, a, b in pairs if not match]
    compared = [(a, b, s) for a, b, s in diff if s is not None]
    miss = [bool(s is None or np.float32(s) < t) for _, s in same]
    fm = [bool(np.float32(s) >= t) for _, _, s in compared]
    sg = [g for g, _ in same]
    return {
        "same_person_pairs": len(same),
        "same_person_pairs_not_compared": sum(1 for _, s in same if s is None),
        "different_person_pairs": len(diff),
        "different_person_pairs_not_compared": rate(len(diff) - len(compared), len(diff)),
        "threshold": threshold,
        "false_match": rate(sum(fm), len(compared), stats.grouped_pair_rate(weights, [a for a, _, _ in compared], [b for _, b, _ in compared], fm)),
        "false_non_match": rate(sum(miss), len(same), stats.grouped_pair_rate(weights, sg, sg, miss)),
        "notes": "A same-person pair with no compared face counts as a miss. A different-person pair with no compared face is not compared, never a true negative.",
    }


def measure_held_out(args, vectors, data, threshold):
    """The manifest cutoff, chosen on LFW, on pairs it never saw."""
    folds = None if args.people is None else {1}
    section, quality = {}, {}
    for name, spec in HELD_OUT.items():
        pairs, verified = fetch_held_out(data, name, folds)
        paths = sorted({str(p) for _, _, a, b in pairs for p in (a, b)})
        rows = vectors.rows(paths, enlarge=ENLARGE)
        chosen = {p: center_face(rows[p]) for p in paths}
        scored, fold_ids = [], []
        for fold, match, a, b in pairs:
            fa, fb = chosen[str(a)], chosen[str(b)]
            score = None
            if is_compared(fa) and is_compared(fb):
                score = float(np.dot(np.asarray(fa["embedding"], dtype=np.float32), np.asarray(fb["embedding"], dtype=np.float32)))
            scored.append((match, score, identity_of(a), identity_of(b)))
            fold_ids.append(fold)
        result = {
            "what": spec["what"],
            "source": f"huggingface.co/datasets/{spec['repo']} at {spec['commit']}",
            "set_digest_verified": verified,
            "pairs": len(pairs),
            "photos": len(paths),
            "at_manifest_threshold": pair_set_statistics(scored, threshold),
        }
        if folds is None:
            scores = np.array([-1.0 if s is None else s for _, s, _, _ in scored])
            mean, std = ten_fold(scores, np.array([m for m, _, _, _ in scored]), np.array(fold_ids))
            result["ten_fold"] = {"accuracy_mean": round(mean, 4), "accuracy_std": round(std, 4)}
        section[name] = result
        quality[name] = gate_summary(rows, paths)
    return section, quality


# Files to scan.


def ffmpeg(*argv):
    subprocess.run(["ffmpeg", "-v", "error", "-y", *map(str, argv)], check=True)


def enlarge_file(source: Path, dest: Path):
    ffmpeg("-i", source, "-vf", "scale=iw*2:ih*2:flags=bicubic", dest)


def read_rgb(path: Path):
    """The first picture of a file as an RGB array."""
    size = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0", str(path)],
        capture_output=True, text=True, check=True,
    ).stdout.strip().splitlines()[0]
    w, h = map(int, size.split(",")[:2])
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(path), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
        capture_output=True, check=True,
    ).stdout
    return np.frombuffer(raw, dtype=np.uint8).reshape(h, w, 3)


def resize_bilinear(image, width, height):
    """A plain bilinear resize, so every clip is the same on every machine."""
    src_h, src_w = image.shape[:2]
    ys = np.clip((np.arange(height) + 0.5) * src_h / height - 0.5, 0, src_h - 1)
    xs = np.clip((np.arange(width) + 0.5) * src_w / width - 0.5, 0, src_w - 1)
    y0, x0 = np.floor(ys).astype(np.int64), np.floor(xs).astype(np.int64)
    y1, x1 = np.minimum(y0 + 1, src_h - 1), np.minimum(x0 + 1, src_w - 1)
    wy, wx = (ys - y0)[:, None, None], (xs - x0)[None, :, None]
    f = image.astype(np.float32)
    top = f[y0][:, x0] * (1 - wx) + f[y0][:, x1] * wx
    bottom = f[y1][:, x0] * (1 - wx) + f[y1][:, x1] * wx
    return np.clip(top * (1 - wy) + bottom * wy + 0.5, 0, 255).astype(np.uint8)


def motion(k, frames, base, origin):
    """Frame k of an appearance: the face grows from 90% to 100% of `base` and
    drifts about 60 px right and 20 px up, with a pixel or two of shake."""
    u = k / (frames - 1) if frames > 1 else 0.0
    scale = 0.9 + 0.1 * u
    x = origin[0] + round(60 * u + 2 * math.sin(0.9 * k))
    y = origin[1] - round(20 * u) + round(2 * math.cos(0.7 * k))
    return scale, x, y


def paste(frame, patch, x, y):
    h, w = patch.shape[:2]
    fh, fw = frame.shape[:2]
    x0, y0, x1, y1 = max(0, x), max(0, y), min(fw, x + w), min(fh, y + h)
    if x1 > x0 and y1 > y0:
        frame[y0:y1, x0:x1] = patch[y0 - y : y1 - y, x0 - x : x1 - x]


def clip_frames(face, canvas, base, origin, on_frames, lead=LEAD_FRAMES, tail=TAIL_FRAMES):
    """A gray screen, the moving face for `on_frames`, then the gray screen again."""
    width, height = canvas
    fh, fw = face.shape[:2]
    for index in range(lead + on_frames + tail):
        frame = np.empty((height, width, 3), dtype=np.uint8)
        frame[:] = BACKGROUND
        k = index - lead
        if 0 <= k < on_frames:
            scale, x, y = motion(k, on_frames, base, origin)
            w = max(1, round(base * scale))
            h = max(1, round(base * scale * fh / fw))
            paste(frame, resize_bilinear(face, w, h), x, y)
        yield frame


def encode(frames, canvas, out: Path, vf=None):
    """Raw frames to H.264 (CRF 23, yuv420p), the way a phone or camera stores video."""
    width, height = canvas
    cmd = ["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{width}x{height}",
           "-r", str(FPS), "-i", "-"]
    if vf:
        cmd += ["-vf", vf]
    cmd += ["-c:v", "libx264", "-crf", "23", "-preset", "veryfast", "-pix_fmt", "yuv420p", str(out)]
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE)
    try:
        for frame in frames:
            proc.stdin.write(frame.tobytes())
    finally:
        proc.stdin.close()
    if proc.wait() != 0:
        raise SystemExit(f"ffmpeg could not encode {out}")


def landscape_clip(face_file: Path, out: Path, on_frames=ON_FRAMES):
    encode(clip_frames(read_rgb(face_file), (1280, 720), 500, (360, 110), on_frames), (1280, 720), out)


def with_orientation(jpeg: bytes, orientation: int) -> bytes:
    """A JPEG with an EXIF orientation tag after its JFIF segment, the way a camera writes it."""
    entry = (0x0112).to_bytes(2, "little") + (3).to_bytes(2, "little") + (1).to_bytes(4, "little")
    entry += orientation.to_bytes(2, "little") + b"\0\0"
    tiff = b"II*\0" + (8).to_bytes(4, "little") + (1).to_bytes(2, "little") + entry + (0).to_bytes(4, "little")
    payload = b"Exif\0\0" + tiff
    segment = b"\xff\xe1" + (len(payload) + 2).to_bytes(2, "big") + payload
    at = 2
    if jpeg[2:4] == b"\xff\xe0":
        at = 4 + int.from_bytes(jpeg[4:6], "big")
    return jpeg[:at] + segment + jpeg[at:]


# Reading a scan report (openworld.result.v2, eval build).


def run_scan(args, pack, source, out, coverage="complete"):
    """`openworld scan` on the CPU, the provider the curve is measured on."""
    done = subprocess.run(
        [args.bin, "--json", "--bundles", args.bundles, "scan", "--input", str(source), "--bundle", args.bundle,
         "--posters", str(pack), "--out", str(out), "--coverage", coverage, "--cpu-only"],
        capture_output=True, text=True,
    )
    lines = done.stdout.strip().splitlines()
    try:
        report = json.loads(lines[-1])
    except (IndexError, json.JSONDecodeError):
        return {"status": "refused", "error": done.stderr.strip()[-400:]}
    require_scores(report)
    return report


def require_scores(report):
    for track in report.get("tracks", []):
        if track.get("status") in COMPARED and not track.get("scores"):
            raise SystemExit("The scan reports no poster scores. Build the openworld command with --features eval.")


def complete(report):
    return report.get("status") == "complete"


def cards(report):
    return {c["poster"]["id"] for c in report.get("candidates", [])}


def scored_tracks(report):
    return [t for t in report.get("tracks", []) if t.get("status") in COMPARED]


def track_scores(track):
    return {s["poster_id"]: s["score"] for s in track.get("scores", [])}


def owner_score(report, owner):
    return max((track_scores(t)[owner] for t in scored_tracks(report) if owner in track_scores(t)), default=None)


def comparisons(report, threshold, owner=None):
    """(comparisons, false matches): each compared face against each poster that is not its own."""
    t = np.float32(threshold)
    n = fm = 0
    for track in scored_tracks(report):
        for poster, score in track_scores(track).items():
            if poster == owner:
                continue
            n += 1
            fm += bool(np.float32(score) >= t)
    return n, fm


def cosine_gap(library_row, report, posters):
    """How far the scan's scores are from the library's cosines on the same still.

    Each compared face of the library is paired with the scan track whose scores
    are closest, and the other way round. Returns (largest gap, face counts
    differ, largest gap between a card and its track)."""
    library = []
    for face in library_row.get("faces", []):
        if face.get("embedding") is not None:
            v = np.array(face["embedding"], dtype=np.float32)
            library.append({pid: float(v @ vec) for pid, vec in posters.items()})
    scan = [track_scores(t) for t in scored_tracks(report)]

    def diff(a, b):
        if set(a) != set(b):
            return 1.0
        return max((abs(a[k] - b[k]) for k in a), default=0.0)

    gap = 0.0
    for side, other in ((library, scan), (scan, library)):
        for a in side:
            if other:
                gap = max(gap, min(diff(a, b) for b in other))
    by_track = {t["track_id"]: track_scores(t) for t in scored_tracks(report)}
    card_gap = 0.0
    for card in report.get("candidates", []):
        own = by_track.get(card["track_id"], {}).get(card["poster"]["id"])
        card_gap = max(card_gap, 1.0 if own is None or "score" not in card else abs(card["score"] - own))
    return gap, len(library) != len(scan), card_gap


def summarize_stills(probes, reports, library, posters, threshold):
    """Stills through `openworld scan`. `probes` is (owner poster or None, file)."""
    found = missed = wrong = no_face = 0
    strangers = carded = 0
    genuine = 0
    n_comp = n_fm = 0
    not_complete = mismatched = 0
    worst = worst_card = 0.0
    for (owner, path), report in zip(probes, reports, strict=True):
        if not complete(report):
            not_complete += 1
            continue
        gap, differ, card_gap = cosine_gap(library[str(path)], report, posters)
        worst, worst_card = max(worst, gap), max(worst_card, card_gap)
        mismatched += differ
        n, fm = comparisons(report, threshold, owner)
        n_comp += n
        n_fm += fm
        got = cards(report)
        if owner is None:
            strangers += 1
            carded += bool(got)
        else:
            genuine += 1
            found += owner in got
            missed += owner not in got
            wrong += bool(got - {owner})
            no_face += not scored_tracks(report)
    return {
        "enrolled_found": rate(found, genuine),
        "enrolled_missed": rate(missed, genuine),
        "enrolled_without_a_compared_face": no_face,
        "enrolled_wrong_poster": rate(wrong, genuine),
        "stranger_any_candidate": rate(carded, strangers),
        "comparisons_with_other_posters": rate(n_fm, n_comp),
        "not_complete": not_complete,
        "face_count_differs_from_library": mismatched,
        "largest_cosine_gap_vs_library": round(worst, 7),
        "largest_card_score_gap_vs_track": round(worst_card, 7),
    }


def appearance(report, owner, threshold):
    """What a scan did with the owner's face: candidate, too_brief, below_cutoff, not_compared, or not_seen."""
    statuses = {t.get("status") for t in report.get("tracks", [])}
    score = owner_score(report, owner)
    passing = score is not None and bool(np.float32(score) >= np.float32(threshold))
    for status in ("candidate", "too_brief", "below_cutoff", "not_compared", "unscored"):
        if status in statuses:
            return status, passing
    return "not_seen", passing


def summarize_video(enrolled, strangers, brief, threshold):
    """`enrolled`: (owner, {coverage: report}); `strangers`: reports; `brief`: (owner, report)."""
    section = {"clips": len(enrolled), "seconds_on_screen": ON_FRAMES / FPS, "frames_per_second": FPS}
    shown = range(LEAD_FRAMES, LEAD_FRAMES + ON_FRAMES)
    for coverage in ("complete", "measured"):
        runs = [(owner, out[coverage]) for owner, out in enrolled]
        hits = [(owner, r) for owner, r in runs if complete(r) and owner in cards(r)]
        on_time = sum(all(c["frame_index"] in shown for c in r["candidates"] if c["poster"]["id"] == owner) for owner, r in hits)
        section[coverage] = {
            "found": rate(len(hits), len(runs)),
            "wrong_poster": sum(bool(cards(r) - {owner}) for owner, r in runs),
            "not_complete": sum(not complete(r) for _, r in runs),
            "card_on_the_shown_second": rate(on_time, len(hits)),
        }
    n = fm = 0
    for r in strangers:
        a, b = comparisons(r, threshold)
        n, fm = n + a, fm + b
    section["stranger_clips"] = {
        "what": "A face that is on no poster, on screen for one second; every poster is a stranger to it.",
        "clips": len(strangers),
        "any_candidate": rate(sum(bool(cards(r)) for r in strangers if complete(r)), sum(complete(r) for r in strangers)),
        "template_comparisons": rate(fm, n),
        "not_complete": sum(not complete(r) for r in strangers),
    }
    outcomes = [appearance(r, owner, threshold) for owner, r in brief]
    held = sum(1 for status, passing in outcomes if passing and status == "too_brief")
    section["two_frame_appearance"] = {
        "what": f"The enrolled face on screen for {BRIEF_FRAMES} frames ({BRIEF_FRAMES / FPS:.3f} s), complete coverage.",
        "clips": len(outcomes),
        "outcomes": dict(sorted(Counter(status for status, _ in outcomes).items())),
        "too_brief": rate(sum(status == "too_brief" for status, _ in outcomes), len(outcomes)),
        "candidate": rate(sum(status == "candidate" for status, _ in outcomes), len(outcomes)),
        "held_back_when_over_the_cutoff": rate(held, sum(passing for _, passing in outcomes)),
        "not_complete": sum(not complete(r) for _, r in brief),
    }
    return section


PHONE_FILES = {
    "jpeg_stored_sideways_with_tag": "A JPEG stored a quarter turn off, with the orientation tag that shows it upright.",
    "photo_12mp_large_face": "A 4032x3024 JPEG, the face about 560 px.",
    "photo_12mp_group_face": "A 4032x3024 JPEG, the face about 190 px, as in a group photo.",
    "portrait_video_with_rotation": "A 1080x1920 H.264 clip stored as 1920x1080 with a display rotation, the face moving on screen for one second.",
}


def summarize_phone(runs):
    """`runs`: (file kind, coverage, owner, report, owner score on the plain still)."""
    section = {}
    for name, what in PHONE_FILES.items():
        section[name] = {"what": what}
        for coverage in sorted({c for n, c, _, _, _ in runs if n == name}):
            rows = [(owner, r, plain) for n, c, owner, r, plain in runs if n == name and c == coverage]
            changes = sorted(owner_score(r, owner) - plain for owner, r, plain in rows if plain is not None and owner_score(r, owner) is not None)
            entry = {
                "found": rate(sum(complete(r) and owner in cards(r) for owner, r, _ in rows), len(rows)),
                "wrong_poster": sum(bool(cards(r) - {owner}) for owner, r, _ in rows),
                "not_complete": sum(not complete(r) for _, r, _ in rows),
                "detection_long_side": sorted({r.get("detection_long_side") for _, r, _ in rows if complete(r)}),
            }
            if changes:
                entry["score_change_vs_plain_still"] = {
                    "compared": len(changes),
                    "mean": sum(changes) / len(changes),
                    "median": changes[len(changes) // 2],
                    "worst": changes[0],
                }
            section[name][coverage] = entry
    return section


def measure_scans(args, vectors, data, threshold, workdir: Path):
    """Stills and clips through `openworld scan`, against a pack of other LFW photos."""
    rng = random.Random(7)
    people = sorted(p for p in (data / "lfw").iterdir() if p.is_dir())
    pairs = [p for p in people if len(list(p.glob("*.jpg"))) >= 2]
    singles = [p for p in people if len(list(p.glob("*.jpg"))) == 1]
    enrolled = rng.sample(pairs, args.scan_people)
    strangers = rng.sample(singles, args.scan_people)
    listing = [f"{p.name}\tmissing\t{p.name}\t{sorted(p.glob('*.jpg'))[0]}" for p in enrolled]
    (workdir / "posters.tsv").write_text("\n".join(listing) + "\n")
    pack = workdir / "pack"
    made = subprocess.run(
        [args.eval_bin, "pack", "--bundles", args.bundles, "--bundle", args.bundle, "--list", str(workdir / "posters.tsv"),
         "--out", str(pack), "--enlarge", str(ENLARGE), "--source", "LFW evaluation"],
        capture_output=True, text=True,
    )
    if made.returncode != 0:
        raise SystemExit(f"openworld-eval pack failed: {made.stderr.strip()}")
    made = json.loads(made.stdout)
    posters = {p["id"]: np.array(p["embedding"], dtype=np.float32) for p in json.loads((pack / "snapshot.json").read_text())["posters"]}
    # A person whose poster photo had no compared face is not on a poster. Their
    # second photo is left out rather than counted as a stranger or a miss.
    probes = [(p.name, sorted(p.glob("*.jpg"))[1]) for p in enrolled if p.name in posters]
    probes += [(None, sorted(p.glob("*.jpg"))[0]) for p in strangers]
    stills = workdir / "stills"
    stills.mkdir(parents=True, exist_ok=True)
    files = []
    for index, (_, photo) in enumerate(probes):
        dest = stills / f"probe-{index:04d}.png"
        enlarge_file(photo, dest)
        files.append(dest)
    library = vectors.rows([str(f) for f in files])
    jobs = ThreadPoolExecutor(args.jobs)
    reports = list(jobs.map(lambda i: run_scan(args, pack, files[i], workdir / f"out-{i}"), range(len(files))))
    owned = [(o, f) for (o, _), f in zip(probes, files, strict=True)]
    section = {
        "posters": len(posters),
        "posters_left_out": made["left_out"],
        "stills": summarize_stills(owned, reports, library, posters, threshold),
    }

    # Clips and phone files use people whose plain still was found, so a miss
    # there is the video path or the file, not the comparison again.
    found = [i for i, (owner, _) in enumerate(owned) if owner and complete(reports[i]) and owner in cards(reports[i])]
    stranger_files = [f for owner, f in owned if owner is None]
    clips = workdir / "clips"
    clips.mkdir(exist_ok=True)

    def enrolled_clip(i):
        owner, face = owned[i]
        clip = clips / f"enrolled-{i:04d}.mp4"
        landscape_clip(face, clip)
        return owner, {c: run_scan(args, pack, clip, clips / f"enrolled-{i}-{c}", c) for c in ("complete", "measured")}

    def stranger_clip(k):
        clip = clips / f"stranger-{k:04d}.mp4"
        landscape_clip(stranger_files[k], clip)
        return run_scan(args, pack, clip, clips / f"stranger-{k}")

    def brief_clip(i):
        owner, face = owned[i]
        clip = clips / f"brief-{i:04d}.mp4"
        landscape_clip(face, clip, BRIEF_FRAMES)
        return owner, run_scan(args, pack, clip, clips / f"brief-{i}")

    chosen = found[: args.video_people]
    section["video"] = summarize_video(
        list(jobs.map(enrolled_clip, chosen)),
        list(jobs.map(stranger_clip, range(min(args.video_people, len(stranger_files))))),
        list(jobs.map(brief_clip, chosen)),
        threshold,
    )
    variants = found[: args.variant_people] if args.variant_people is not None else found
    section["phone_files"] = summarize_phone([run for out in jobs.map(lambda i: phone_files(args, pack, owned[i], reports[i], workdir, i), variants) for run in out])
    jobs.shutdown()
    providers = {r.get("provider") for r in reports if r.get("provider")}
    return section, providers


def phone_files(args, pack, probe, plain_report, workdir: Path, index):
    """One found person, in the files a phone makes. The face and poster are the
    ones that matched in the plain still, so a miss here is the decode, the
    turn, or the detection size."""
    owner, source = probe
    out = workdir / "phone"
    out.mkdir(exist_ok=True)
    plain = owner_score(plain_report, owner)
    made = []
    sideways = out / f"{index}-sideways.jpg"
    ffmpeg("-i", source, "-vf", "transpose=2", "-q:v", "2", sideways)
    sideways.write_bytes(with_orientation(sideways.read_bytes(), 6))
    made.append(("jpeg_stored_sideways_with_tag", sideways, "complete"))
    for name, size in (("photo_12mp_large_face", 1500), ("photo_12mp_group_face", 500)):
        photo = out / f"{index}-{name}.jpg"
        ffmpeg("-i", source, "-vf", f"scale={size}:{size}:flags=bicubic,pad=4032:3024:1400:600:color=0x6b7280,setsar=1",
               "-q:v", "3", photo)
        made.append((name, photo, "complete"))
    stored = out / f"{index}-stored.mp4"
    clip = out / f"{index}-portrait.mp4"
    # Upright portrait frames, stored a quarter turn clockwise, then tagged so a player turns them back.
    encode(clip_frames(read_rgb(source), (1080, 1920), 820, (100, 520), ON_FRAMES), (1080, 1920), stored, vf="transpose=1")
    ffmpeg("-display_rotation", "90", "-i", stored, "-c", "copy", clip)
    made += [("portrait_video_with_rotation", clip, "complete"), ("portrait_video_with_rotation", clip, "measured")]
    runs = []
    for n, (name, path, coverage) in enumerate(made):
        report = run_scan(args, pack, path, out / f"{index}-{name}-{n}", coverage)
        runs.append((name, coverage, owner, report, plain))
    return runs


# WIDER FACE.


def wider_boxes(data: Path):
    lines = (data / "wider_face_split" / "wider_face_val_bbx_gt.txt").read_text().split("\n")
    out, i = {}, 0
    while i < len(lines) and lines[i].strip():
        name = lines[i].strip()
        count = int(lines[i + 1])
        rows = lines[i + 2 : i + 2 + max(count, 1)]
        boxes = []
        for row in rows[:count]:
            x, y, w, h, blur, expression, illumination, invalid, occlusion, pose = map(int, row.split()[:10])
            if not invalid and w > 0 and h > 0:
                boxes.append({"rect": (x, y, w, h), "occlusion": occlusion, "pose": pose, "blur": blur})
        out[name] = boxes
        i += 2 + max(count, 1)
    return out


def iou(a, b):
    ax, ay, aw, ah = a
    bx, by, bw, bh = b
    ix = max(0, min(ax + aw, bx + bw) - max(ax, bx))
    iy = max(0, min(ay + ah, by + bh) - max(ay, by))
    inter = ix * iy
    union = aw * ah + bw * bh - inter
    return inter / union if union else 0.0


WIDER_BANDS = {
    "faces_112px_and_up": "Labeled short side at least 112 px on the original image: the size a face is compared at.",
    "faces_64_to_112px": "Labeled short side 64 to 111 px on the original image: seen, but under the compare size.",
    "faces_64px_and_up_on_the_detection_image": "Labeled short side at least 64 px after the scan's resize for the detector.",
}


def wider_statistics(truth, rows):
    """Detection recall by face size and occlusion, and what the gates did with found faces.

    A labeled face counts as found when a detection overlaps it at IoU 0.5 or more."""
    groups = {band: {g: [0, 0] for g in ("all", "not_occluded", "occluded", "typical_pose")} for band in WIDER_BANDS}
    gates = Counter()
    off = off_compared = detections = 0
    for name, row in rows.items():
        faces = row.get("faces", [])
        detections += len(faces)
        for face in faces:
            if all(iou(face["rect"], b["rect"]) < 0.3 for b in truth[name]):
                off += 1
                off_compared += is_compared(face)
        long = max(row["width"], row["height"])
        scale = row.get("detection_long_side", long) / long
        for box in truth[name]:
            short = min(box["rect"][2], box["rect"][3])
            best = max(faces, key=lambda f: iou(f["rect"], box["rect"]), default=None)
            hit = best is not None and iou(best["rect"], box["rect"]) >= 0.5
            bands = []
            if short >= 112:
                bands.append("faces_112px_and_up")
                if hit:
                    gates[outcome(best)] += 1
            elif short >= 64:
                bands.append("faces_64_to_112px")
            if short * scale >= 64:
                bands.append("faces_64px_and_up_on_the_detection_image")
            for band in bands:
                for g, keep in (("all", True), ("not_occluded", box["occlusion"] == 0), ("occluded", box["occlusion"] > 0),
                                ("typical_pose", box["pose"] == 0)):
                    if keep:
                        groups[band][g][0] += hit
                        groups[band][g][1] += 1
    section = {"images": len(rows), "match_rule": "IoU at least 0.5 with a detection."}
    for band, what in WIDER_BANDS.items():
        section[band] = {"what": what, **{g: rate(*v) for g, v in groups[band].items()}}
    section["detections"] = detections
    section["detections_off_every_labeled_face"] = off
    section["compared_detections_off_every_labeled_face"] = off_compared
    quality = {"found_faces_112px_and_up": sum(gates.values()), "outcome": dict(sorted(gates.items()))}
    return section, quality


def measure_wider(args, vectors, data):
    truth = wider_boxes(data)
    names = sorted(truth)
    if args.wider_images:
        names = names[:: max(1, len(names) // args.wider_images)][: args.wider_images]
    paths = {n: str(data / "WIDER_val" / "images" / n) for n in names}
    rows = vectors.rows(list(paths.values()))
    return wider_statistics({n: truth[n] for n in names}, {n: rows[paths[n]] for n in names if "error" not in rows[paths[n]]})


# This machine.


def hardware(args, vector_providers, scan_providers):
    bench = subprocess.run([args.bin, "--json", "--bundles", args.bundles, "bench", "--bundle", args.bundle],
                           capture_output=True, text=True)
    try:
        bench = json.loads(bench.stdout.strip().splitlines()[-1])
    except (IndexError, json.JSONDecodeError):
        bench = {"error": bench.stderr.strip()[-400:]}
    ort = os.environ.get("OPENWORLD_ORT_LIB")
    return {
        "measured_on": "CPU. The library's vectors and every scan ran with ONNX Runtime's CPU provider.",
        "vectors_provider": sorted(vector_providers),
        "scan_provider": sorted(scan_providers),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "cpus": os.cpu_count(),
        "python": platform.python_version(),
        "numpy": np.__version__,
        "onnxruntime_library_sha256": sha256_file(Path(ort)) if ort and Path(ort).is_file() else None,
        "bench": bench,
        "bench_note": "openworld bench: per-provider timings, and the lowest cosine between an accelerated provider and the CPU. A provider whose scores drift is not described by this curve.",
    }


# The curve and its checks.


NOT = [
    "LFW, CALFW, and CPLFW are celebrity press photos. They are not mugshots, not FBI posters, not CCTV, and not the phone photos a scan reads. NIST FRTE numbers are not this curve.",
    "LFW has no demographic labels, so there is no subgroup analysis here: false match and miss rates by age, sex, or skin tone are not measured. Face recognition error rates are known to differ across groups.",
    "LFW faces are about 94 px and are enlarged 2x. This measures faces near the 112 px compare size, not larger or sharper ones, and not tiny or blurred ones (the gates refuse those).",
    "Identities overlap across LFW, CALFW, and CPLFW, and the embedder may have been trained on similar celebrity photos. The held-out sets test other photos and conditions, not other people.",
    "The clips are built from stills: a face drifting and growing on a gray screen, H.264 at CRF 23. They are not real footage with motion blur, lighting changes, or crowds.",
    "A video template averages up to five frames. Its false match rate is checked only on the stranger clips here; the per-comparison rate is the single-photo LFW rate.",
    "Wilson intervals treat pairs as independent. The identity bootstrap resamples people. Neither covers a different population or a different camera.",
    "This curve does not allow real FBI posters. real_posters_allowed is false.",
]


def manifest_threshold(bundles, bundle):
    manifest = tomllib.loads((Path(bundles) / bundle / "manifest.toml").read_text())
    return float(manifest["threshold"]), manifest


def sample_rates(curve, published):
    """(name, the sample's entry, the published rate) for every rate a sample can be held to."""
    out = []

    def add(name, sample, pub):
        out.append((name, sample, pub))

    lfw, plfw = curve["lfw"], published["lfw"]
    for kind in ("false_match", "false_non_match", "false_non_match_one_pair_per_person"):
        add(f"LFW {kind} at the manifest threshold", lfw["at_manifest_threshold"][kind], plfw["at_manifest_threshold"][kind])
    for op in plfw["operating_points"]:
        if op.get("threshold") is None:
            continue
        here = lfw["at_thresholds"].get(f"{op['threshold']:.4f}")
        for kind in ("false_match", "false_non_match"):
            add(f"LFW {kind} at {op['threshold']:.4f} (published FMR {op['target_fmr']:g})", here and here[kind], op[kind])
    for key in ("lfw_enlarged_2x", "lfw_native_size"):
        add(f"{key} center face compared", curve["quality"][key]["center_face_compared"], published["quality"][key]["center_face_compared"])
    for name, section in curve["held_out_pairs"].items():
        here, pub = section["at_manifest_threshold"], published["held_out_pairs"][name]["at_manifest_threshold"]
        for kind in ("false_match", "false_non_match", "different_person_pairs_not_compared"):
            add(f"{name.upper()} {kind}", here[kind], pub[kind])
    stills, pstills = curve["scans"]["stills"], published["scans"]["stills"]
    for kind in ("enrolled_found", "enrolled_wrong_poster", "stranger_any_candidate", "comparisons_with_other_posters"):
        add(f"stills {kind}", stills[kind], pstills[kind])
    video, pvideo = curve["scans"]["video"], published["scans"]["video"]
    for coverage in ("complete", "measured"):
        add(f"video found, {coverage}", video[coverage]["found"], pvideo[coverage]["found"])
    add("stranger clips with a card", video["stranger_clips"]["any_candidate"], pvideo["stranger_clips"]["any_candidate"])
    for kind in ("too_brief", "candidate"):
        add(f"two-frame appearance {kind}", video["two_frame_appearance"][kind], pvideo["two_frame_appearance"][kind])
    for name, runs in curve["scans"]["phone_files"].items():
        for coverage, entry in runs.items():
            if coverage != "what":
                pub = published["scans"]["phone_files"].get(name, {}).get(coverage)
                add(f"{name}, {coverage}", entry["found"], pub and pub["found"])
    for band in WIDER_BANDS:
        for g in ("all", "not_occluded", "occluded"):
            add(f"WIDER {band} {g}", curve["wider_face_val"][band][g], published["wider_face_val"][band][g])
    return out


def invariants(curve):
    """What must hold on any run, sample or full."""
    failures = []
    scans = curve["scans"]
    stills, video = scans["stills"], scans["video"]
    if stills["largest_cosine_gap_vs_library"] > COSINE_TOLERANCE:
        failures.append(f"the scan's scores are {stills['largest_cosine_gap_vs_library']:.6f} from the library's cosines")
    if stills["largest_card_score_gap_vs_track"] > COSINE_TOLERANCE:
        failures.append("a card's score is not its track's score")
    if stills["face_count_differs_from_library"]:
        failures.append(f"{stills['face_count_differs_from_library']} stills: the scan compared a different number of faces than the library")
    if stills["stranger_any_candidate"]["count"]:
        failures.append(f"{stills['stranger_any_candidate']['count']} stranger stills got a card")
    if stills["enrolled_wrong_poster"]["count"]:
        failures.append(f"{stills['enrolled_wrong_poster']['count']} enrolled stills carded another person's poster")
    if video["stranger_clips"]["any_candidate"]["count"]:
        failures.append(f"{video['stranger_clips']['any_candidate']['count']} stranger clips got a card")
    if video["two_frame_appearance"]["candidate"]["count"]:
        failures.append("a face on screen for two frames became a candidate")
    for coverage in ("complete", "measured"):
        if video[coverage]["wrong_poster"]:
            failures.append(f"video, {coverage}: {video[coverage]['wrong_poster']} clips carded another person's poster")
    not_complete = stills["not_complete"] + video["stranger_clips"]["not_complete"] + video["two_frame_appearance"]["not_complete"]
    not_complete += sum(video[c]["not_complete"] for c in ("complete", "measured"))
    for name, runs in scans["phone_files"].items():
        for coverage, entry in runs.items():
            if coverage != "what":
                not_complete += entry["not_complete"]
                if entry["wrong_poster"]:
                    failures.append(f"{name}, {coverage}: {entry['wrong_poster']} carded another person's poster")
    if not_complete:
        failures.append(f"{not_complete} scans did not complete")
    return failures


def full_run_rules(curve):
    """A full run sets the cutoff. The manifest may not sit under it."""
    failures = []
    cutoff = curve["measured_cutoff"]
    if cutoff is None:
        failures.append("the LFW cutoff could not be measured")
    elif curve["threshold"] < cutoff - 1e-9:
        failures.append(f"the manifest threshold {curve['threshold']} is under the measured cutoff {cutoff} (FMR {CUTOFF_FMR:g})")
    return failures


def operating_threshold(curve, target):
    for op in curve["lfw"]["operating_points"]:
        if op["target_fmr"] == target:
            return op.get("threshold")
    return None


def check(curve, published):
    """A run against the published curve: the same operating point, and every rate inside its own interval."""
    failures = []
    for key, want in (("schema", CURVE_SCHEMA), ("real_posters_allowed", False), ("picker_line", PICKER_LINE)):
        if published.get(key) != want:
            failures.append(f"the published curve's {key} is {published.get(key)!r}, not {want!r}")
    if published.get("bundle") != curve["bundle"]:
        failures.append(f"the published curve is for {published.get('bundle')}, this run is {curve['bundle']}")
    if abs(curve["threshold"] - published["threshold"]) > 1e-9:
        failures.append(f"the manifest threshold {curve['threshold']} is not the published {published['threshold']}")
    if published.get("measured_cutoff") is None or published["threshold"] < published["measured_cutoff"] - 1e-9:
        failures.append("the published threshold is under the published measured cutoff")
    published_fmr = published["lfw"]["at_manifest_threshold"]["false_match"]["rate"]
    if published.get("chance", {}).get("fmr_at_cutoff") != published_fmr:
        failures.append("the published chance.fmr_at_cutoff is not the published LFW false match rate at the threshold")
    here, there = operating_threshold(curve, CHECK_FMR), operating_threshold(published, CHECK_FMR)
    if here is None or there is None:
        failures.append(f"the score at FMR {CHECK_FMR:g} is missing ({here} here, {there} published)")
    elif abs(here - there) > CHECK_SCORE_TOLERANCE:
        failures.append(f"the score at FMR {CHECK_FMR:g} is {here:.4f} here and {there:.4f} published")
    for name, sample, pub in sample_rates(curve, published):
        if sample is None or pub is None:
            failures.append(f"{name}: missing ({'here' if sample is None else 'in the published curve'})")
        elif not pub["trials"]:
            continue
        elif not sample["trials"]:
            failures.append(f"{name}: no trials here, {pub['trials']} published")
        elif not stats.agrees(pub["rate"], sample):
            lo, hi = stats.interval(sample)
            failures.append(f"{name}: {sample['count']}/{sample['trials']} here; the published {pub['rate']:.6g} is outside [{lo:.6g}, {hi:.6g}]")
    return failures


def build_curve(bundle, manifest, threshold, cutoff, sample, lfw, held_out, scans, wider, quality, hardware_info):
    """The curve file. `sample` names the sample sizes, or is None for a full run."""
    fm = lfw["at_manifest_threshold"]["false_match"]
    upper = stats.upper95(fm)
    return {
        "schema": CURVE_SCHEMA,
        "bundle": bundle,
        "bundle_version": manifest.get("version"),
        "models": manifest.get("models"),
        "weights": {f["role"]: f["sha256"] for f in manifest.get("files", [])},
        "threshold": threshold,
        "picker_line": PICKER_LINE,
        "real_posters_allowed": False,
        "cutoff_rule": f"Lowest LFW all-pairs score with a false match rate at or under {CUTOFF_FMR:g}, on faces enlarged 2x. The manifest threshold may not be under it.",
        "measured_cutoff": cutoff,
        "sample": sample,
        "chance": {
            "fmr_at_cutoff": fm["rate"],
            "fmr_upper95": upper,
            "source": CHANCE_SOURCE,
            "threshold": threshold,
            "what": "A scan compares every compared face with every enabled poster. Chance alone gives about FMR x posters x faces false candidates.",
            "per_scan": stats.chance_rows(fm["rate"] or 0.0, upper),
        },
        "lfw": lfw,
        "held_out_pairs": held_out,
        "scans": scans,
        "wider_face_val": wider,
        "quality": quality,
        "hardware": hardware_info,
        "sources": {
            "lfw": "Labeled Faces in the Wild, all 13,233 images, pairs.txt view 2. Downloaded, not committed.",
            "wider_face": "WIDER FACE validation, 3,226 images. Downloaded, not committed.",
            "held_out": "CALFW and CPLFW pairs, read from a mirror at a pinned commit. Downloaded, not committed.",
            "pins": {**PINS, **{f"{k} set": v for k, v in HELD_OUT_PINS.items()}},
            "urls": SOURCES,
        },
        "not": NOT,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin", required=True, help="the openworld command, built with --features eval")
    parser.add_argument("--eval-bin", required=True, help="openworld-eval from eval/real")
    parser.add_argument("--bundles", default=str(ROOT / "bundles"))
    parser.add_argument("--bundle", default="fast")
    parser.add_argument("--data", default=str(Path.home() / ".cache" / "openworld-eval"))
    parser.add_argument("--out", help="write the curve here")
    parser.add_argument("--check", help="fail unless this run agrees with this published curve")
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 2)
    parser.add_argument("--people", type=int, help="LFW identities to sample; every identity and the ten folds when absent")
    parser.add_argument("--wider-images", type=int, help="WIDER validation images to sample; all when absent")
    parser.add_argument("--scan-people", type=int, default=100)
    parser.add_argument("--video-people", type=int, default=20)
    parser.add_argument("--variant-people", type=int, help="found enrolled people to put in phone-made files; all when absent")
    args = parser.parse_args()
    data = Path(args.data)
    fetch(data, list(SOURCES))
    threshold, manifest = manifest_threshold(args.bundles, args.bundle)
    published = json.loads(Path(args.check).read_text()) if args.check else None
    # A sample is also measured at the published operating points, to compare like with like.
    extra = [op["threshold"] for op in published["lfw"]["operating_points"] if op.get("threshold") is not None] if published else []
    vectors = Vectors(args.eval_bin, args.bundles, args.bundle, data / "cache", args.jobs)
    lfw, quality, cutoff = measure_lfw(args, vectors, data, threshold, extra)
    print(f"LFW cutoff at FMR {CUTOFF_FMR:g}: {cutoff}; manifest {threshold}", file=sys.stderr)
    with tempfile.TemporaryDirectory() as tmp:
        scans, scan_providers = measure_scans(args, vectors, data, threshold, Path(tmp))
    wider, quality["wider_face_val"] = measure_wider(args, vectors, data)
    held_out, held_quality = measure_held_out(args, vectors, data, threshold)
    quality.update(held_quality)
    sample = {"people": args.people, "wider_images": args.wider_images, "scan_people": args.scan_people,
              "video_people": args.video_people, "variant_people": args.variant_people} if args.people is not None else None
    curve = build_curve(args.bundle, manifest, threshold, cutoff, sample, lfw, held_out, scans, wider, quality,
                        hardware(args, vectors.providers, scan_providers))
    text = json.dumps(curve, indent=2) + "\n"
    if args.out:
        Path(args.out).write_text(text)
    print(text)
    failures = invariants(curve)
    if sample is None:
        failures += full_run_rules(curve)
    if published is not None:
        failures += check(curve, published)
    if failures:
        raise SystemExit("\n".join(failures))
    print("real faces ok", file=sys.stderr)


if __name__ == "__main__":
    main()
