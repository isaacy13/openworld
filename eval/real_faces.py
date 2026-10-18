#!/usr/bin/env python3
"""Measure a network bundle on public face photographs.

The photographs are downloaded into --data. None of them are committed. Only
counts, rates, and score thresholds are written to --out.

LFW (Huang et al., 2007) measures the face comparison: the standard ten-fold
pairs, and every same-person and different-person pair, so a false match rate
near one in a million has trials behind it. WIDER FACE validation (Yang et al.,
2016) measures detection at the 64 px rule. Stills and compressed videos built
from LFW go through the shipping `openworld scan` command, against a poster
pack built from other LFW photos, and the cosines the scan reports are checked
against the library's own vectors.

CALFW and CPLFW are held out: the cutoff chosen on LFW is applied to their
pairs unchanged, so their miss and false match rates were not tuned on them.

LFW faces are about 94 px (median detected short side; 85 to 104 px for
the middle 80%), under the 112 px compare rule. They are enlarged
twice with a bicubic filter. The embedder still reads a 112 px aligned crop,
so this measures faces about that size, not larger ones.
"""

import argparse
import csv
import hashlib
import json
import math
import os
import random
import subprocess
import sys
import tempfile
import time
import urllib.request
import zipfile
import tarfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
SOURCES = {
    "lfw.tgz": "https://ndownloader.figshare.com/files/5976018",
    "pairs.txt": "https://ndownloader.figshare.com/files/5976006",
    "WIDER_val.zip": "https://huggingface.co/datasets/CUHK-CSE/wider_face/resolve/main/data/WIDER_val.zip",
    "wider_face_split.zip": "https://huggingface.co/datasets/CUHK-CSE/wider_face/resolve/main/data/wider_face_split.zip",
}
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
FMR_TARGETS = [1e-3, 1e-4, 1e-5, 1e-6]
# A scan compares each face with every enabled poster. One in a hundred
# thousand per comparison keeps a pack of a thousand posters near one false
# candidate per hundred faces.
CUTOFF_FMR = 1e-5


def wilson(k, n, z=1.96):
    if n == 0:
        return [0.0, 1.0]
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return [max(0.0, c - h), min(1.0, c + h)]


def rate(k, n):
    return {"count": int(k), "trials": int(n), "rate": (k / n) if n else None, "wilson95": wilson(k, n)}


def fetch(data: Path, need):
    data.mkdir(parents=True, exist_ok=True)
    for name in need:
        target = data / name
        if not target.exists():
            print(f"downloading {name}", file=sys.stderr)
            download(SOURCES[name], target)
        digest = hashlib.sha256()
        with target.open("rb") as handle:
            while block := handle.read(1 << 20):
                digest.update(block)
        if digest.hexdigest() != PINS[name]:
            target.unlink()
            raise SystemExit(f"{name} is not the pinned file. Deleted it. Refusing.")
    if "lfw.tgz" in need and not (data / "lfw").is_dir():
        with tarfile.open(data / "lfw.tgz") as archive:
            archive.extractall(data, filter="data")
    if "WIDER_val.zip" in need and not (data / "WIDER_val").is_dir():
        zipfile.ZipFile(data / "WIDER_val.zip").extractall(data)
    if "wider_face_split.zip" in need and not (data / "wider_face_split").is_dir():
        zipfile.ZipFile(data / "wider_face_split.zip").extractall(data)


def download(url: str, target: Path, attempts=6):
    target.parent.mkdir(parents=True, exist_ok=True)
    part = target.with_suffix(target.suffix + ".part")
    for attempt in range(attempts):
        try:
            with urllib.request.urlopen(url, timeout=120) as response, part.open("wb") as out:
                while block := response.read(1 << 20):
                    out.write(block)
            part.replace(target)
            return
        except OSError:
            if attempt == attempts - 1:
                raise
            time.sleep(2 ** (attempt + 1))


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


def fetch_held_out(data: Path, name, folds=None):
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
        with ThreadPoolExecutor(8) as pool:
            list(pool.map(lambda p: download(f"{base}/{spec['folder']}/{p.relative_to(root / spec['folder']).as_posix()}", p), missing))
    if folds is None:
        digest = hashlib.sha256()
        for path in [root / "pairs.csv", *photos]:
            digest.update(path.relative_to(root).as_posix().encode())
            digest.update(hashlib.sha256(path.read_bytes()).digest())
        got, want = digest.hexdigest(), HELD_OUT_PINS[name]
        if want is None:
            print(f"{name} digest {got} is not pinned yet", file=sys.stderr)
        elif got != want:
            raise SystemExit(f"{name} is not the pinned set ({got}). Refusing.")
    return pairs


def measure_held_out(args, vectors, data, threshold):
    """The manifest cutoff, chosen on LFW, on pairs it never saw.

    A same-person pair where either photo has no compared face is a miss: the
    scan would not have carded it. A different-person pair without one is not a
    false match."""
    folds = None if args.people is None else {1}
    section = {}
    for name, spec in HELD_OUT.items():
        pairs = fetch_held_out(data, name, folds)
        paths = sorted({str(p) for _, _, a, b in pairs for p in (a, b)})
        rows = vectors.rows(paths, enlarge=2)
        chosen = {p: center_face(rows[p]) for p in paths}
        scores, same, fold_ids = [], [], []
        no_face = {"same": 0, "different": 0}
        for fold, match, a, b in pairs:
            fa, fb = chosen[str(a)], chosen[str(b)]
            if fa is None or fb is None:
                no_face["same" if match else "different"] += 1
                score = -1.0
            else:
                score = float(np.dot(fa["embedding"], fb["embedding"]))
            scores.append(score)
            same.append(match)
            fold_ids.append(fold)
        scores, same, fold_ids = np.array(scores), np.array(same), np.array(fold_ids)
        genuine, impostor = scores[same], scores[~same]
        result = {
            "what": spec["what"],
            "source": f"huggingface.co/datasets/{spec['repo']} at {spec['commit']}",
            "pairs": len(pairs),
            "photos": len(paths),
            "photo_without_a_compared_face": rate(sum(c is None for c in chosen.values()), len(paths)),
            "same_pairs_without_a_face": no_face["same"],
            "different_pairs_without_a_face": no_face["different"],
            "at_manifest_threshold": {
                "threshold": threshold,
                "false_match": rate(int((impostor >= threshold).sum()), len(impostor)),
                "false_non_match": rate(int((genuine < threshold).sum()), len(genuine)),
            },
        }
        if folds is None:
            mean, std = ten_fold(scores, same, fold_ids)
            result["ten_fold"] = {"accuracy_mean": round(mean, 4), "accuracy_std": round(std, 4)}
        section[name] = result
    return section


class Vectors:
    """Runs `openworld-eval vectors` in parallel shards and caches the rows."""

    def __init__(self, eval_bin, bundles, bundle, cache: Path, jobs):
        self.eval_bin, self.bundles, self.bundle, self.cache, self.jobs = eval_bin, bundles, bundle, cache, jobs
        manifest = (Path(bundles) / bundle / "manifest.toml").read_text()
        # The vectors depend on the weights and the tool, not on the threshold.
        weights = "".join(l for l in manifest.splitlines() if l.startswith("sha256"))
        tool = hashlib.sha256(Path(eval_bin).read_bytes()).hexdigest()
        self.key = hashlib.sha256((weights + tool).encode()).hexdigest()[:16]
        cache.mkdir(parents=True, exist_ok=True)

    def rows(self, paths, detection="640", enlarge=1):
        tag = hashlib.sha256(("\n".join(map(str, paths)) + detection + str(enlarge) + self.key).encode()).hexdigest()[:16]
        cached = self.cache / f"vectors-{tag}.jsonl"
        if cached.is_file():
            return {row["path"]: row for row in map(json.loads, cached.read_text().splitlines())}
        shards = [paths[i :: self.jobs] for i in range(self.jobs)]

        def run(shard):
            if not shard:
                return []
            with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as handle:
                handle.write("\n".join(map(str, shard)))
            try:
                out = subprocess.run(
                    [self.eval_bin, "vectors", "--bundles", self.bundles, "--bundle", self.bundle,
                     "--list", handle.name, "--detection", detection, "--enlarge", str(enlarge)],
                    capture_output=True, text=True, check=True,
                ).stdout
            finally:
                os.unlink(handle.name)
            return out.splitlines()

        with ThreadPoolExecutor(self.jobs) as pool:
            lines = [line for part in pool.map(run, shards) for line in part]
        cached.write_text("\n".join(lines) + "\n")
        return {row["path"]: row for row in map(json.loads, lines)}


def center_face(row):
    """LFW centers the labeled person. Of the compared faces, that is the nearest one."""
    faces = [f for f in row.get("faces", []) if f["embedding"] is not None]
    if not faces:
        return None
    cx, cy = row["width"] / 2, row["height"] / 2

    def distance(f):
        x, y, w, h = f["rect"]
        return (x + w / 2 - cx) ** 2 + (y + h / 2 - cy) ** 2

    return min(faces, key=distance)


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


def all_pair_curve(embeddings, identities):
    """Every same-person and every different-person pair, counted in fine score bins."""
    bins = np.linspace(-1.0, 1.0, 20001)
    genuine = np.zeros(len(bins) - 1, dtype=np.int64)
    impostor = np.zeros(len(bins) - 1, dtype=np.int64)
    n = len(embeddings)
    ids = np.asarray(identities)
    block = 2048
    for start in range(0, n, block):
        part = embeddings[start : start + block] @ embeddings.T
        for row in range(part.shape[0]):
            i = start + row
            scores = part[row, i + 1 :]
            same = ids[i + 1 :] == ids[i]
            genuine += np.histogram(scores[same], bins)[0]
            impostor += np.histogram(scores[~same], bins)[0]
    return bins, genuine, impostor


def at_or_above(counts, bins, threshold):
    index = np.searchsorted(bins, threshold, side="left")
    return int(counts[min(index, len(counts)) :].sum())


def cutoff_for(bins, impostor, target):
    total = impostor.sum()
    above = np.cumsum(impostor[::-1])[::-1]
    for index in range(len(above)):
        if above[index] / total <= target:
            return float(bins[index])
    return 1.0


def enlarge_file(source: Path, dest: Path):
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(source), "-vf", "scale=iw*2:ih*2:flags=bicubic", str(dest)], check=True)


def scan(bin_path, bundles, bundle, posters, source, out, coverage="complete"):
    result = subprocess.run(
        [bin_path, "--json", "--bundles", bundles, "scan", "--input", str(source), "--bundle", bundle,
         "--posters", str(posters), "--out", str(out), "--coverage", coverage, "--long-side", "640"],
        capture_output=True, text=True,
    )
    return json.loads(result.stdout)


def first_pair_scores(found, chosen):
    """One same-person score per identity: its first two compared photos.

    The all-pairs miss rate is weighted by how often a person was photographed.
    A pack holds one poster per person, so this is the miss rate a person sees."""
    by_person = {}
    for p in found:
        by_person.setdefault(Path(p).parent.name, []).append(p)
    scores = []
    for photos in by_person.values():
        if len(photos) >= 2:
            a, b = sorted(photos)[:2]
            scores.append(float(np.dot(chosen[a]["embedding"], chosen[b]["embedding"])))
    return np.array(scores)


def measure_lfw(args, vectors, data, threshold):
    images = lfw_images(data, args.people)
    paths = [str(p) for _, p in images]
    enlarged = vectors.rows(paths, enlarge=2)
    native = vectors.rows(paths, enlarge=1)
    chosen = {p: center_face(enlarged[p]) for p in paths}
    found = [p for p in paths if chosen[p] is not None]
    native_seen = sum(1 for p in paths if native[p].get("faces"))
    native_compared = sum(1 for p in paths if center_face(native[p]) is not None)
    embeddings = np.array([chosen[p]["embedding"] for p in found], dtype=np.float32)
    identities = [Path(p).parent.name for p in found]
    bins, genuine, impostor = all_pair_curve(embeddings, identities)
    g_total, i_total = int(genuine.sum()), int(impostor.sum())
    per_person = first_pair_scores(found, chosen)

    def point(t):
        return {
            "threshold": round(t, 4),
            "false_match": rate(at_or_above(impostor, bins, t), i_total),
            "false_non_match": rate(g_total - at_or_above(genuine, bins, t), g_total),
            "false_non_match_one_pair_per_person": rate(int((per_person < t).sum()), len(per_person)),
        }

    operating = [{"target_fmr": target, **point(cutoff_for(bins, impostor, target))} for target in FMR_TARGETS]
    cutoff = cutoff_for(bins, impostor, CUTOFF_FMR)
    section = {
        "images": len(paths),
        "identities": len({Path(p).parent.name for p in paths}),
        "enlarged_2x": {
            "center_face_compared": rate(len(found), len(paths)),
            "genuine_pairs": g_total,
            "impostor_pairs": i_total,
            "operating_points": operating,
            "at_manifest_threshold": point(threshold),
        },
        "native_size": {
            "face_seen": rate(native_seen, len(paths)),
            "face_compared": rate(native_compared, len(paths)),
        },
    }
    if args.people is None:
        pairs = lfw_pairs(data)
        scores, same, folds, missing = [], [], [], 0
        for fold, match, a, b in pairs:
            fa, fb = chosen.get(str(lfw_path(data, *a))), chosen.get(str(lfw_path(data, *b)))
            if fa is None or fb is None:
                missing += 1
                score = -1.0
            else:
                score = float(np.dot(fa["embedding"], fb["embedding"]))
            scores.append(score)
            same.append(match)
            folds.append(fold)
        scores, same, folds = np.array(scores), np.array(same), np.array(folds)
        mean, std = ten_fold(scores, same, folds)
        section["enlarged_2x"]["ten_fold"] = {
            "pairs": len(pairs),
            "pairs_without_a_face": missing,
            "accuracy_mean": round(mean, 4),
            "accuracy_std": round(std, 4),
        }
    return section, cutoff, chosen


def measure_scans(args, vectors, data, threshold, workdir: Path):
    """Stills and videos through `openworld scan`, against a pack of other LFW photos."""
    rng = random.Random(7)
    people = sorted(p for p in (data / "lfw").iterdir() if p.is_dir())
    pairs = [p for p in people if len(list(p.glob("*.jpg"))) >= 2]
    singles = [p for p in people if len(list(p.glob("*.jpg"))) == 1]
    enrolled = rng.sample(pairs, args.scan_people)
    strangers = rng.sample(singles, args.scan_people)
    stills = workdir / "stills"
    stills.mkdir(parents=True, exist_ok=True)
    listing = []
    probes = []
    for person in enrolled:
        photos = sorted(person.glob("*.jpg"))
        listing.append(f"{person.name}\tmissing\t{person.name}\t{photos[0]}")
        probes.append((person.name, photos[1]))
    for person in strangers:
        probes.append((None, sorted(person.glob("*.jpg"))[0]))
    (workdir / "posters.tsv").write_text("\n".join(listing) + "\n")
    pack = workdir / "pack"
    made = subprocess.run(
        [args.eval_bin, "pack", "--bundles", args.bundles, "--bundle", args.bundle, "--list", str(workdir / "posters.tsv"),
         "--out", str(pack), "--enlarge", "2", "--source", "LFW evaluation"],
        capture_output=True, text=True, check=True,
    )
    pack_posters = json.loads(made.stdout)["posters"]
    files = []
    for index, (_, photo) in enumerate(probes):
        dest = stills / f"probe-{index:04d}.png"
        if not dest.exists():
            enlarge_file(photo, dest)
        files.append(dest)
    reference = vectors.rows([str(f) for f in files])
    poster_vectors = {p["id"]: np.array(p["embedding"]) for p in json.loads((pack / "snapshot.json").read_text())["posters"]}

    def run_still(index):
        return index, scan(args.bin, args.bundles, args.bundle, pack, files[index], workdir / f"out-{index}")

    with ThreadPoolExecutor(args.jobs) as pool:
        reports = dict(pool.map(run_still, range(len(files))))
    tp = fn = fp_genuine = fp_stranger = refused = 0
    genuine_trials = stranger_trials = 0
    worst_gap = 0.0
    for index, (owner, _) in enumerate(probes):
        report = reports[index]
        if report["status"] != "complete":
            refused += 1
            continue
        face = center_face(reference[str(files[index])])
        if face is not None:
            # A still can hold more than one face. The center face's cosine is one of the rows for that poster.
            probe = np.array(face["embedding"])
            for poster_id, vector in poster_vectors.items():
                rows = [row["cosine"] for row in report["comparisons"] if row["poster_id"] == poster_id]
                expected = float(probe @ vector)
                gap = min((abs(expected - c) for c in rows), default=1.0)
                worst_gap = max(worst_gap, gap)
        cards = {c["poster_id"] for c in report["candidates"] if c["kind"] == "face"}
        if owner is None:
            stranger_trials += 1
            fp_stranger += bool(cards)
        else:
            genuine_trials += 1
            tp += owner in cards
            fn += owner not in cards
            fp_genuine += bool(cards - {owner})
    section = {
        "posters": pack_posters,
        "stills": {
            "enrolled_found": rate(tp, genuine_trials),
            "enrolled_missed": rate(fn, genuine_trials),
            "enrolled_wrong_poster": rate(fp_genuine, genuine_trials),
            "stranger_any_candidate": rate(fp_stranger, stranger_trials),
            "not_complete": refused,
            "largest_cosine_gap_vs_library": round(worst_gap, 6),
        },
    }
    section["video"] = measure_videos(args, enrolled, strangers, files, probes, reports, pack, workdir)
    return section


def measure_videos(args, enrolled, strangers, files, probes, reports, pack, workdir: Path):
    """A 1280x720 H.264 clip: a stranger for two seconds, the enrolled person for
    one second, then the stranger again. Only people whose still was found are used,
    so this measures the video path, not the comparison again."""
    found = [i for i, (owner, _) in enumerate(probes) if owner and owner in {c["poster_id"] for c in reports[i]["candidates"]}]
    chosen = found[: args.video_people]
    stranger_files = files[len(enrolled) :]
    results = {"complete": [0, 0], "measured": [0, 0]}
    frames_ok = 0

    def build(index):
        owner = probes[index][0]
        other = stranger_files[index % len(stranger_files)]
        clip = workdir / f"clip-{index:04d}.mp4"
        if not clip.exists():
            graph = (
                "[0:v]scale=500:500,pad=1280:720:390:110,setsar=1,fps=30,trim=duration=2[a];"
                "[1:v]scale=500:500,pad=1280:720:390:110,setsar=1,fps=30,trim=duration=1[b];"
                "[2:v]scale=500:500,pad=1280:720:390:110,setsar=1,fps=30,trim=duration=2[c];"
                "[a][b][c]concat=n=3:v=1:a=0,format=yuv420p[v]"
            )
            subprocess.run(
                ["ffmpeg", "-v", "error", "-y", "-loop", "1", "-i", str(other), "-loop", "1", "-i", str(files[index]),
                 "-loop", "1", "-i", str(other), "-filter_complex", graph, "-map", "[v]", "-c:v", "libx264",
                 "-crf", "23", "-preset", "veryfast", str(clip)],
                check=True,
            )
        out = {}
        for coverage in ("complete", "measured"):
            out[coverage] = scan(args.bin, args.bundles, args.bundle, pack, clip, workdir / f"vout-{index}-{coverage}", coverage)
        return owner, out

    with ThreadPoolExecutor(args.jobs) as pool:
        built = list(pool.map(build, chosen))
    for owner, out in built:
        for coverage, report in out.items():
            results[coverage][1] += 1
            hits = [c for c in report["candidates"] if c["kind"] == "face" and c["poster_id"] == owner]
            if report["status"] == "complete" and hits:
                results[coverage][0] += 1
                if all(60 <= c["frame_index"] < 90 for c in hits):
                    frames_ok += 1
    return {
        "clips": len(chosen),
        "seconds_on_screen": 1.0,
        "complete_found": rate(*results["complete"]),
        "measured_found": rate(*results["measured"]),
        "cards_on_the_shown_second": rate(frames_ok, results["complete"][0] + results["measured"][0]),
    }


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


def measure_wider(args, vectors, data):
    truth = wider_boxes(data)
    names = sorted(truth)
    if args.wider_images:
        names = names[:: max(1, len(names) // args.wider_images)][: args.wider_images]
    paths = [str(data / "WIDER_val" / "images" / n) for n in names]
    section = {"images": len(paths)}
    for detection in ("640", "full"):
        rows = vectors.rows(paths, detection=detection)
        groups = {"all": [0, 0], "no_occlusion": [0, 0], "frontal": [0, 0]}
        unmatched = detections = 0
        for name, path in zip(names, paths):
            row = rows[path]
            long = max(row["width"], row["height"])
            scale = 1.0 if detection == "full" else min(1.0, 640 / long)
            faces = [f["rect"] for f in row["faces"]]
            detections += len(faces)
            for face in faces:
                if all(iou(face, b["rect"]) < 0.3 for b in truth[name]):
                    unmatched += 1
            for box in truth[name]:
                short = min(box["rect"][2], box["rect"][3]) * scale
                # The rule is 64 px. A box a few pixels over it can come back a few under.
                if short < 72:
                    continue
                hit = any(iou(face, box["rect"]) >= 0.5 for face in faces)
                for group, keep in (("all", True), ("no_occlusion", box["occlusion"] == 0), ("frontal", box["pose"] == 0)):
                    if keep:
                        groups[group][0] += hit
                        groups[group][1] += 1
        section[f"detection_{detection}"] = {
            "recall_faces_72px_and_up": {g: rate(*v) for g, v in groups.items()},
            "detections": detections,
            "detections_off_every_labeled_face": unmatched,
        }
    return section


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin", required=True, help="the openworld command")
    parser.add_argument("--eval-bin", required=True, help="openworld-eval from eval/real")
    parser.add_argument("--bundles", default=str(ROOT / "bundles"))
    parser.add_argument("--bundle", default="open")
    parser.add_argument("--data", default=str(Path.home() / ".cache" / "openworld-eval"))
    parser.add_argument("--out", help="write the curve here")
    parser.add_argument("--check", help="fail if the measured cutoff or the scans disagree with this curve")
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 2)
    parser.add_argument("--people", type=int, help="LFW identities to sample; every identity and the ten folds when absent")
    parser.add_argument("--wider-images", type=int, help="WIDER validation images to sample; all when absent")
    parser.add_argument("--scan-people", type=int, default=100)
    parser.add_argument("--video-people", type=int, default=20)
    args = parser.parse_args()
    data = Path(args.data)
    fetch(data, list(SOURCES))
    manifest = (Path(args.bundles) / args.bundle / "manifest.toml").read_text()
    threshold = float(next(l.split("=")[1] for l in manifest.splitlines() if l.startswith("threshold")))
    vectors = Vectors(args.eval_bin, args.bundles, args.bundle, data / "cache", args.jobs)
    lfw, cutoff, _ = measure_lfw(args, vectors, data, threshold)
    print(f"LFW cutoff at FMR {CUTOFF_FMR:g}: {cutoff:.4f}; manifest {threshold}", file=sys.stderr)
    with tempfile.TemporaryDirectory() as tmp:
        scans = measure_scans(args, vectors, data, threshold, Path(tmp))
    wider = measure_wider(args, vectors, data)
    held_out = measure_held_out(args, vectors, data, threshold)
    curve = {
        "schema": "openworld.curve.v1",
        "bundle": args.bundle,
        "picker_line": "LFW curve measured. Real FBI photos stay off.",
        "real_posters_allowed": False,
        "threshold": threshold,
        "cutoff_rule": f"Lowest LFW all-pairs score with a false match rate at or under {CUTOFF_FMR:g}.",
        "measured_cutoff": round(cutoff, 4),
        "lfw": lfw,
        "held_out_pairs": held_out,
        "scans": scans,
        "wider_face_val": wider,
        "sources": {
            "lfw": "Labeled Faces in the Wild, all 13,233 images, pairs.txt view 2. Downloaded, not committed.",
            "wider_face": "WIDER FACE validation, 3,226 images. Downloaded, not committed.",
            "held_out": "CALFW and CPLFW pairs, read from a mirror at a pinned commit. Downloaded, not committed.",
        },
        "not": "Not mugshots, not FBI posters, not NIST. LFW is celebrity press photos.",
    }
    text = json.dumps(curve, indent=2) + "\n"
    if args.out:
        Path(args.out).write_text(text)
    print(text)
    if args.check:
        failures = check(curve, json.loads(Path(args.check).read_text()), args)
        if failures:
            raise SystemExit("\n".join(failures))
        print("real faces ok", file=sys.stderr)


def rates(curve):
    """The rates whose trials are independent, so a sample has an interval around them."""
    lfw = curve["lfw"]["enlarged_2x"]["at_manifest_threshold"]
    stills, video = curve["scans"]["stills"], curve["scans"]["video"]
    held = {
        f"{name.upper()} {kind.replace('_', ' ')}": section["at_manifest_threshold"][kind]
        for name, section in curve["held_out_pairs"].items()
        for kind in ("false_match", "false_non_match")
    }
    return {
        **held,
        "LFW miss, one pair per person": lfw["false_non_match_one_pair_per_person"],
        "enrolled stills found": stills["enrolled_found"],
        "video found, complete": video["complete_found"],
        "video found, measured": video["measured_found"],
        "WIDER recall at 640": curve["wider_face_val"]["detection_640"]["recall_faces_72px_and_up"]["all"],
    }


def check(curve, published, args):
    failures = []
    if abs(curve["threshold"] - published["threshold"]) > 1e-9:
        failures.append("the manifest cutoff is not the published one")
    stills = curve["scans"]["stills"]
    if stills["not_complete"]:
        failures.append(f"{stills['not_complete']} scans did not complete")
    if stills["largest_cosine_gap_vs_library"] > 1e-4:
        failures.append("the scan's cosines are not the library's")
    if stills["enrolled_wrong_poster"]["count"]:
        failures.append(f"{stills['enrolled_wrong_poster']['count']} enrolled stills carded another poster")
    if stills["stranger_any_candidate"]["count"] > 2:
        failures.append(f"strangers matched {stills['stranger_any_candidate']['count']}")
    if args.people is not None:
        # A sample is a few dozen trials. It has to agree with the full run, not clear the floors.
        for name, sample in rates(curve).items():
            want = rates(published)[name]["rate"]
            lo, hi = sample["wilson95"]
            if sample["trials"] and want is not None and not lo <= want <= hi:
                failures.append(f"{name}: {sample['count']}/{sample['trials']} here, the published {want:.4f} is outside [{lo:.4f}, {hi:.4f}]")
        return failures
    # Regression guards a little past the published run, not targets. Missing an
    # older or a turned photo is common at this cutoff; a false match is not.
    floors = {
        "CALFW false match": (None, 0.001),
        "CALFW false non match": (None, 0.42),
        "CPLFW false match": (None, 0.001),
        "CPLFW false non match": (None, 0.65),
        "LFW miss, one pair per person": (None, 0.08),
        "enrolled stills found": (0.9, None),
        "video found, complete": (0.9, None),
        "video found, measured": (0.9, None),
        "WIDER recall at 640": (0.9, None),
    }
    for name, value in rates(curve).items():
        low, high = floors.get(name, (None, None))
        if (low is not None and value["rate"] < low) or (high is not None and value["rate"] > high):
            failures.append(f"{name} {value['rate']:.4f}")
    if not 0 <= curve["threshold"] - curve["measured_cutoff"] <= 0.01:
        failures.append(f"the manifest's {curve['threshold']} is not at or just above the measured cutoff {curve['measured_cutoff']}")
    return failures


if __name__ == "__main__":
    main()
