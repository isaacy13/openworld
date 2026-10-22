"""The FBI poster list and the poster-photo measurement for eval/real_faces.py.

The list is classified with the same rules as the core's `classify_item`
(core/openworld-core/src/update.rs). Both are tested on the same items in
eval/fbi_classify_cases.json, so the two copies cannot drift apart.

The poster photos are downloaded into --data. None of them are committed, and
nothing about one person reaches the curve: no uid, name, URL, caption, or
photo. Only counts, rates, intervals, and thresholds do.
"""

import hashlib
import json
import re
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from itertools import combinations

import numpy as np

import stats
from stats import rate

FBI_LIST = "https://api.fbi.gov/wanted/v1/list"
PAGE_SIZE = 50
# A pause before every list page and every photo, per download worker.
PAUSE_SECONDS = 1.0
DOWNLOAD_WORKERS = 2
MAX_PAGES = 200
# A few thousand photos at most: every poster's first photo, then second photos, and so on.
PHOTOS_PER_POSTER = 5
PHOTO_LIMIT = 4000
PHOTO_MAX_BYTES = 30 << 20
# Two photos of one poster this close are one picture saved twice, not two photos.
NEAR_DUPLICATE = 0.99
# The poster's own words. They are what the FBI wrote, not how a person describes themselves.
DEMOGRAPHIC_FIELDS = ("sex", "race", "race_raw")
IMPOSTOR_SETS = ("poster_x_poster", "poster_x_lfw")
AGE_PROGRESSED = re.compile(r"progress|enhanced", re.IGNORECASE)
# "AMINA AND BELEL KANDIL": one poster, two people. Its photos are not a same-person pair.
MORE_THAN_ONE_PERSON = re.compile(r"\bAND\b|&|/")


# The core's classify_item, rule for rule.

RESOLVED = ("captured", "located", "recovered", "deceased", "surrendered", "found", "resolved")
# Rust's `str::trim` removes Unicode White_Space, which is not Python's `str.strip()` set.
RUST_WHITESPACE = "".join(map(chr, [0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x20, 0x85, 0xA0, 0x1680, *range(0x2000, 0x200B),
                                    0x2028, 0x2029, 0x202F, 0x205F, 0x3000]))
# Rust's `to_ascii_uppercase` and `to_ascii_lowercase` change ASCII letters only.
ASCII_UPPER = {c: c - 32 for c in range(ord("a"), ord("z") + 1)}
ASCII_LOWER = {c: c + 32 for c in range(ord("A"), ord("Z") + 1)}


def text_field(item, key):
    """`item.get(key).and_then(|v| v.as_str())`: a JSON string, or None."""
    value = item.get(key) if isinstance(item, dict) else None
    return value if isinstance(value, str) else None


def unnamed(title_up):
    t = title_up.strip(RUST_WHITESPACE)
    return not t or t == "SEEKING INFORMATION" or t == "UNKNOWN" or "UNIDENTIFIED" in t or "REMAINS" in t


def classify(item):
    """(poster, None) for a kept item, or (None, why it was skipped). The order of the rules is the core's."""
    status = (text_field(item, "status") or "").translate(ASCII_LOWER)
    if any(word in status for word in RESOLVED):
        return None, "resolved"
    classification = text_field(item, "poster_classification") or ""
    title = (text_field(item, "title") or "").strip(RUST_WHITESPACE)
    class_up, title_up = classification.translate(ASCII_UPPER), title.translate(ASCII_UPPER)
    if "ECAP" in class_up:
        return None, "ecap"
    if "UNIDENTIFIED" in class_up or "UNIDENTIFIED" in title_up or "REMAINS" in title_up:
        return None, "unidentified"
    seeking = "SEEKING" in class_up
    if seeking and unnamed(title_up):
        return None, "unnamed_seeking_information"
    if "MISSING" in class_up:
        poster_class = "missing"
    elif "WANTED" in class_up or seeking:
        poster_class = "wanted"
    else:
        return None, "not_missing_or_wanted"
    if unnamed(title_up):
        return None, "unnamed"
    uid = text_field(item, "uid")
    url = text_field(item, "url")
    url = url if url is not None else "https://www.fbi.gov/wanted"
    if not (url.startswith("https://www.fbi.gov/") or url.startswith("https://fbi.gov/")):
        return None, "not_an_fbi_page"
    return {"id": uid if uid is not None else title, "class": poster_class, "title": title, "fbi_url": url}, None


def classify_item(item):
    return classify(item)[0]


# The list and its photos.


def fbi_host(url):
    """An https URL on fbi.gov or one of its subdomains."""
    if not isinstance(url, str):
        return False
    try:
        parts = urllib.parse.urlsplit(url)
    except ValueError:
        return False
    host = (parts.hostname or "").lower()
    return parts.scheme == "https" and (host == "fbi.gov" or host.endswith(".fbi.gov"))


class FbiOnlyRedirects(urllib.request.HTTPRedirectHandler):
    """A photo URL on fbi.gov may not redirect off it."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        if not fbi_host(newurl):
            raise urllib.error.HTTPError(newurl, code, "redirect off fbi.gov refused", headers, fp)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def fbi_opener():
    return urllib.request.build_opener(FbiOnlyRedirects)


def fetch_list(get, pause=PAUSE_SECONDS, page_size=PAGE_SIZE, max_pages=MAX_PAGES, sleep=time.sleep):
    """Every item of the list, page by page, until the `total` the first page states.

    `get(url)` returns the body. An item already read (the same uid) on a later
    page is not counted twice; the list can shift while it is paged. Returns
    (items, total)."""
    items, seen, total = [], set(), None
    for page in range(1, max_pages + 1):
        if page > 1:
            sleep(pause)
        body = json.loads(get(f"{FBI_LIST}?page={page}&pageSize={page_size}"))
        if total is None:
            total = int(body.get("total") or 0)
        got = body.get("items") or []
        for item in got:
            key = text_field(item, "uid") or json.dumps(item, sort_keys=True)
            if key not in seen:
                seen.add(key)
                items.append(item)
        if not got or len(items) >= total:
            break
    return items, total or 0


def demographics(item):
    """The poster's raw sex and race fields. An empty or missing field is `unknown`."""
    out = {}
    for field in DEMOGRAPHIC_FIELDS:
        value = item.get(field) if isinstance(item, dict) else None
        if value is not None and not isinstance(value, str):
            value = json.dumps(value, sort_keys=True)
        value = (value or "").strip()
        out[field] = value or "unknown"
    return out


def photo_refs(item, per_poster=PHOTOS_PER_POSTER):
    """The poster's photos, largest size first, up to `per_poster`. Returns (refs, photos past the limit)."""
    images = item.get("images") if isinstance(item, dict) else None
    refs = []
    for image in images if isinstance(images, list) else []:
        if not isinstance(image, dict):
            continue
        url = image.get("original") or image.get("large")
        caption = image.get("caption") if isinstance(image.get("caption"), str) else ""
        refs.append({"url": url if isinstance(url, str) else None, "age_progressed": bool(AGE_PROGRESSED.search(caption))})
    return refs[:per_poster], max(0, len(refs) - per_poster)


def summarize_list(items, total):
    """The kept posters, and the counts the curve may carry about the list."""
    kept, skipped, class_kept, class_skipped = [], Counter(), Counter(), Counter()
    past_limit = 0
    for item in items:
        poster, why = classify(item)
        raw = (text_field(item, "poster_classification") or "").strip() or "none"
        if poster is None:
            skipped[why] += 1
            class_skipped[raw] += 1
            continue
        class_kept[raw] += 1
        poster["groups"] = demographics(item)
        poster["images"], extra = photo_refs(item)
        past_limit += extra
        kept.append(poster)
    summary = {
        "total": int(total),
        "items_read": len(items),
        "complete": bool(total) and len(items) >= total,
        "kept": dict(sorted(Counter(p["class"] for p in kept).items())),
        "skipped": dict(sorted(skipped.items())),
        "poster_classification_kept": dict(sorted(class_kept.items())),
        "poster_classification_skipped": dict(sorted(class_skipped.items())),
        "photos_listed": sum(len(p["images"]) for p in kept) + past_limit,
        "photos_past_the_per_poster_limit": past_limit,
        "per_poster_limit": PHOTOS_PER_POSTER,
    }
    return kept, summary


def sample_posters(kept, n):
    """Every k-th poster by id, so a sample is the same on every machine for the same list."""
    ordered = sorted(kept, key=lambda p: p["id"])
    if n is None or n >= len(ordered):
        return ordered
    return ordered[:: max(1, len(ordered) // n)][:n]


def plan_downloads(kept, limit=PHOTO_LIMIT):
    """Every poster's first photo, then every second photo, and so on, up to `limit`.

    A photo off fbi.gov is never fetched. Each photo left out gets its `outcome` here."""
    depth = max((len(p["images"]) for p in kept), default=0)
    order = [p["images"][k] for k in range(depth) for p in kept if k < len(p["images"])]
    todo = []
    for image in order:
        if not image["url"]:
            image["outcome"] = "no_url"
        elif not fbi_host(image["url"]):
            image["outcome"] = "off_fbi_gov"
        elif len(todo) >= limit:
            image["outcome"] = "over_the_photo_limit"
        else:
            todo.append(image)
    return todo


def extension(body):
    if body[:3] == b"\xff\xd8\xff":
        return ".jpg"
    if body[:8] == b"\x89PNG\r\n\x1a\n":
        return ".png"
    if body[:4] == b"RIFF" and body[8:12] == b"WEBP":
        return ".webp"
    if body[:4] == b"GIF8":
        return ".gif"
    return ".bin"


def download_photos(kept, folder, get, pause=PAUSE_SECONDS, workers=DOWNLOAD_WORKERS, limit=PHOTO_LIMIT, sleep=time.sleep):
    """Each planned photo, saved under its own SHA-256 so a changed photo is a new file.

    Sets `path` and `sha256` on a downloaded photo, `outcome` on one that failed."""
    todo = plan_downloads(kept, limit)
    folder.mkdir(parents=True, exist_ok=True)

    def one(image):
        sleep(pause)
        try:
            body = get(image["url"])
        except OSError:
            image["outcome"] = "download_failed"
            return
        digest = hashlib.sha256(body).hexdigest()
        path = folder / f"{digest}{extension(body)}"
        if not path.exists():
            part = path.with_suffix(path.suffix + ".part")
            part.write_bytes(body)
            part.replace(path)
        image["sha256"], image["path"] = digest, str(path)

    with ThreadPoolExecutor(max(1, workers)) as pool:
        list(pool.map(one, todo))
    return todo


# The measurement.


def normalized_name(title):
    return " ".join(re.sub(r"[^A-Z0-9]+", " ", title.translate(ASCII_UPPER)).split())


def names_more_than_one(title):
    return bool(MORE_THAN_ONE_PERSON.search(title.translate(ASCII_UPPER)))


def person_clusters(posters):
    """One id per person: posters with the same name, or the same photo file, are one person.

    The same person can be on two posters (a wanted poster and a kidnapping
    poster, or a poster that was reissued). Their pairs are left out, not
    counted as different people."""
    parent = list(range(len(posters)))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    first = {}
    for i, poster in enumerate(posters):
        keys = [("photo", image["sha256"]) for image in poster["images"] if image.get("sha256")]
        name = normalized_name(poster["title"])
        if name:
            keys.append(("name", name))
        for key in keys:
            if key in first:
                parent[find(i)] = find(first[key])
            else:
                first[key] = i
    roots = [find(i) for i in range(len(posters))]
    if not roots:
        return np.zeros(0, dtype=np.int64)
    return np.unique(roots, return_inverse=True)[1].astype(np.int64)


def poster_reason(poster):
    """Why a poster has no compared face: `no_photo_listed`, or its photos' most common outcome."""
    outcomes = Counter(image.get("outcome", "not_measured") for image in poster["images"])
    if not outcomes:
        return "no_photo_listed"
    return max(sorted(outcomes), key=outcomes.get)


def pairs_at_or_above(embeddings, groups, threshold, block=1024):
    """Every pair i < j of different groups scoring at or above `threshold`."""
    n = len(embeddings)
    t = np.float32(threshold)
    found = ([], [], [])
    cols = np.arange(n)[None, :]
    for start in range(0, n, block):
        stop = min(n, start + block)
        part = embeddings[start:stop] @ embeddings.T
        keep = (cols > np.arange(start, stop)[:, None]) & (groups[start:stop, None] != groups[None, :]) & (part >= t)
        rows, hit = np.nonzero(keep)
        found[0].append(rows + start)
        found[1].append(hit)
        found[2].append(part[rows, hit])
    return tuple(np.concatenate(x) if x else np.zeros(0) for x in found)


def cross_at_or_above(a, b, threshold, block=512):
    """Every (i in a, j in b) scoring at or above `threshold`."""
    t = np.float32(threshold)
    found = ([], [], [])
    for start in range(0, len(a), block):
        part = a[start : start + block] @ b.T
        rows, hit = np.nonzero(part >= t)
        found[0].append(rows + start)
        found[1].append(hit)
        found[2].append(part[rows, hit])
    return tuple(np.concatenate(x) if x else np.zeros(0) for x in found)


def different_pairs(counts):
    """Pairs of photos of different people, given each person's photo count."""
    counts = np.asarray(counts, dtype=np.int64)
    total = int(counts.sum())
    return (total * total - int((counts * counts).sum())) // 2


def bootstrap_false_match(weights, ga, gb, totals):
    a, b, trials, _ = stats.aggregate_pairs(ga, gb, np.ones(len(ga)))
    return stats.bootstrap_rate(stats.pair_sums(weights, a, b, trials), totals)


def statistics(posters, lfw_embeddings, lfw_names, threshold, replicates=1000, seed=20261019):
    """False matches, misses, and their groups, at `threshold`.

    `posters` are kept posters whose photos carry `outcome` and, when compared,
    `embedding` and `sha256`. `lfw_embeddings` are the public: one compared
    face per LFW photo, `lfw_names` its person."""
    t = np.float32(threshold)
    outcomes, reasons = Counter(), Counter()
    compared, repeated = [], 0
    for poster in posters:
        seen, mine = set(), []
        for image in poster["images"]:
            outcomes[image.get("outcome", "not_measured")] += 1
            if image.get("outcome") != "compared":
                continue
            if image["sha256"] in seen:
                repeated += 1
                continue
            seen.add(image["sha256"])
            mine.append(image)
        if mine:
            compared.append((poster, mine))
        else:
            reasons[poster_reason(poster)] += 1
    lfw = np.ascontiguousarray(lfw_embeddings, dtype=np.float32)
    dim = len(compared[0][1][0]["embedding"]) if compared else (lfw.shape[1] if lfw.ndim == 2 else 1)
    lfw = lfw.reshape(-1, dim) if lfw.size else np.zeros((0, dim), dtype=np.float32)
    cluster = person_clusters([p for p, _ in compared])
    clusters = int(cluster.max()) + 1 if len(cluster) else 0
    owner = np.array([i for i, (_, mine) in enumerate(compared) for _ in mine], dtype=np.int64)
    emb = np.array([image["embedding"] for _, mine in compared for image in mine], dtype=np.float32).reshape(-1, dim)
    img_cluster = cluster[owner] if len(owner) else np.zeros(0, dtype=np.int64)
    img_groups = {f: np.array([compared[o][0]["groups"][f] for o in owner], dtype=object) for f in DEMOGRAPHIC_FIELDS}
    lfw_labels, lfw_ids = np.unique(np.asarray(list(lfw_names), dtype=str), return_inverse=True)
    lfw_ids = lfw_ids.astype(np.int64).reshape(-1)
    lfw_counts = np.bincount(lfw_ids, minlength=len(lfw_labels))

    w_posters = stats.bootstrap_weights(clusters, replicates, seed)
    w_public = stats.bootstrap_weights(len(lfw_labels), replicates, seed + 1)
    w_both = np.hstack([w_posters, w_public])
    public_drawn = w_public @ lfw_counts

    # Posters against posters.
    pi, pj, ps = pairs_at_or_above(emb, img_cluster, t)
    pi, pj = pi.astype(np.int64), pj.astype(np.int64)

    def poster_pairs(mask):
        counts = np.bincount(img_cluster[mask], minlength=clusters)
        hit = mask[pi] & mask[pj]
        boot = bootstrap_false_match(w_posters, img_cluster[pi[hit]], img_cluster[pj[hit]], stats.impostor_totals(w_posters, counts))
        return rate(int(hit.sum()), different_pairs(counts), boot), hit

    # Posters against the public.
    xi, xj, xs = cross_at_or_above(emb, lfw, t)
    xi, xj = xi.astype(np.int64), xj.astype(np.int64)

    def public_pairs(mask):
        counts = np.bincount(img_cluster[mask], minlength=clusters)
        hit = mask[xi]
        totals = (w_posters @ counts) * public_drawn
        boot = bootstrap_false_match(w_both, img_cluster[xi[hit]], clusters + lfw_ids[xj[hit]], totals)
        return rate(int(hit.sum()), int(mask.sum()) * len(lfw), boot), hit

    every = np.ones(len(emb), dtype=bool)
    impostor = {}
    for kind, measure, (ia, ib, scores), what in (
        ("poster_x_poster", poster_pairs, (pi, pj, ps),
         "Every pair of compared poster photos of different people."),
        ("poster_x_lfw", public_pairs, (xi, xj, xs),
         "Every compared poster photo against every compared LFW face (enlarged 2x), the public."),
    ):
        fm, _ = measure(every)
        ga = img_cluster[ia]
        gb = img_cluster[ib] if kind == "poster_x_poster" else clusters + lfw_ids[ib]
        other = ib if kind == "poster_x_poster" else len(emb) + ib
        by_group = {}
        for field in DEMOGRAPHIC_FIELDS:
            values = img_groups[field]
            by_group[field] = {}
            for value in sorted(set(values.tolist())):
                mask = values == value
                entry, _ = measure(mask)
                entry["photos"] = int(mask.sum())
                entry["posters"] = int(len(set(owner[mask].tolist())))
                by_group[field][value] = entry
        impostor[kind] = {
            "what": what,
            "false_match": fm,
            "false_match_spread": stats.false_match_spread(ia, other, ga, gb),
            "false_matches_at_or_above_near_duplicate": int(np.count_nonzero(np.asarray(scores, dtype=np.float32) >= np.float32(NEAR_DUPLICATE))),
            "by_group": by_group,
            "by_group_rule": ("Both posters in the group." if kind == "poster_x_poster" else "The poster in the group, against every LFW face."),
        }

    # One poster's own photos.
    gen_s, gen_g, gen_aged, one_s, one_g = [], [], [], [], []
    more_than_one = near_duplicates = with_two = 0
    for index, (poster, mine) in enumerate(compared):
        if len(mine) < 2:
            continue
        if names_more_than_one(poster["title"]):
            more_than_one += 1
            continue
        with_two += 1
        vectors = [np.asarray(image["embedding"], dtype=np.float32) for image in mine]
        first = True
        for a, b in combinations(range(len(mine)), 2):
            score = np.float32(vectors[a] @ vectors[b])
            if score >= np.float32(NEAR_DUPLICATE):
                near_duplicates += 1
                continue
            gen_s.append(score)
            gen_g.append(int(cluster[index]))
            gen_aged.append(mine[a]["age_progressed"] or mine[b]["age_progressed"])
            if first:
                one_s.append(score)
                one_g.append(int(cluster[index]))
                first = False
    gen_s, gen_aged = np.asarray(gen_s, dtype=np.float32), np.asarray(gen_aged, dtype=bool)
    miss = gen_s < t
    one_miss = np.asarray(one_s, dtype=np.float32) < t

    def missed(mask):
        g = [x for x, keep in zip(gen_g, mask) if keep]
        flags = miss[mask]
        return rate(int(flags.sum()), int(mask.sum()), stats.grouped_pair_rate(w_posters, g, g, flags))

    genuine = {
        "what": ("Pairs of different photos on one poster, which names one person. These include "
                 "age-progressions and photos taken years apart, so a miss here is often an old or drawn face."),
        "posters_with_two_or_more_compared_photos": with_two,
        "posters_left_out_naming_more_than_one_person": more_than_one,
        "pairs_left_out_as_the_same_picture": near_duplicates,
        "near_duplicate_score": NEAR_DUPLICATE,
        "false_non_match": missed(np.ones(len(gen_s), dtype=bool)),
        "false_non_match_one_pair_per_poster": rate(int(one_miss.sum()), len(one_s),
                                                    stats.grouped_pair_rate(w_posters, one_g, one_g, one_miss)),
        "false_non_match_with_an_age_progressed_photo": missed(gen_aged),
        "false_non_match_without_one": missed(~gen_aged),
        "age_progressed_rule": "A photo whose caption says age-progressed or enhanced.",
    }

    pp, pl = impostor["poster_x_poster"]["false_match"], impostor["poster_x_lfw"]["false_match"]
    fmr = max(pp["rate"] or 0.0, pl["rate"] or 0.0)
    upper = max(stats.upper95(pp), stats.upper95(pl))
    section = {
        "threshold": threshold,
        "posters": len(posters),
        "posters_with_a_compared_face": rate(len(compared), len(posters)),
        "posters_without_a_compared_face": {"count": len(posters) - len(compared), "reasons": dict(sorted(reasons.items()))},
        "photos": {
            "measured": sum(len(p["images"]) for p in posters),
            "compared": int(len(emb)),
            "outcomes": dict(sorted(outcomes.items())),
            "same_file_twice_on_a_poster": repeated,
        },
        "people": {
            "posters_compared": len(compared),
            "people_after_merging": clusters,
            "rule": ("Posters with the same name, or the same photo file, are one person: "
                     "their pairs are neither different-person nor same-person pairs."),
        },
        "public": {"lfw_faces": int(len(lfw)), "lfw_people": int(len(lfw_labels))},
        "impostor": impostor,
        "genuine": genuine,
        "chance": {
            "fmr": fmr,
            "fmr_upper95": upper,
            "source": "The higher of the poster x poster and poster x LFW rates, and the higher upper bound.",
            "posters": len(compared),
            "what": ("Expected false candidates in one scan against every poster with a compared face, "
                     "one vector per poster: FMR x posters x compared faces."),
            "per_scan": stats.chance_rows(fmr, upper, posters=(len(compared),)),
        },
        "intervals": (
            f"wilson95 treats pairs as independent. identity_bootstrap95 resamples the {clusters} poster people "
            f"and the {len(lfw_labels)} LFW people {replicates} times (seed {seed}), so a person's pairs move together."
        ),
    }
    return section
