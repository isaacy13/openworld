"""Counting and intervals for eval/real_faces.py. Pure numpy, no photographs.

Scores are float32 cosines, compared in float32 the way the scan compares them
(`score >= cutoff`). A cutoff is a point on a 0.0001 grid.

Two intervals are reported for a rate over face pairs:

- Wilson, which treats every pair as independent.
- An identity bootstrap, which resamples people, not pairs. One person's
  photos appear in many pairs, so their errors move together. When a few
  people cause most of the false matches, this interval is the honest one.

For a bootstrap replicate, identity `a` drawn `w_a` times gives a pair of
different people (a, b) the weight `w_a * w_b`, and a pair of one person's
photos the weight `w_a`. Pairs between two copies of one person are left out.
"""

import math
from statistics import NormalDist

import numpy as np

GRID = 10000  # cutoffs are multiples of 1 / GRID
Z95 = 1.959963984540054


def wilson(k, n, z=Z95):
    """95% Wilson interval for k of n."""
    if n == 0:
        return [0.0, 1.0]
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(max(0.0, p * (1 - p) / n + z * z / (4 * n * n))) / d
    return [max(0.0, c - h), min(1.0, c + h)]


def rate(k, n, bootstrap=None):
    """A count, its trials, the rate, and its intervals."""
    out = {"count": int(k), "trials": int(n), "rate": (k / n) if n else None, "wilson95": wilson(k, n)}
    if bootstrap is not None:
        out["identity_bootstrap95"] = [float(bootstrap[0]), float(bootstrap[1])]
    return out


def interval(entry):
    """The widest 95% interval a rate entry carries."""
    lo, hi = entry["wilson95"]
    if "identity_bootstrap95" in entry:
        blo, bhi = entry["identity_bootstrap95"]
        lo, hi = min(lo, blo), max(hi, bhi)
    return lo, hi


def upper95(entry):
    return interval(entry)[1]


def f32(t):
    return np.float32(t)


def count_at_or_above(scores, t):
    return int(np.count_nonzero(np.asarray(scores, dtype=np.float32) >= f32(t)))


def count_below(scores, t):
    return int(np.count_nonzero(np.asarray(scores, dtype=np.float32) < f32(t)))


def grid_above(score):
    """The lowest grid point that `score` is under, compared in float32 as the scan compares."""
    s = np.float32(score)
    k = math.floor(float(s) * GRID)
    while k > -GRID and np.float32(k / GRID) > s:
        k -= 1
    while np.float32(k / GRID) <= s:
        k += 1
    return round(k / GRID, 4)


def cutoff_from_tail(tail_desc, total, target):
    """The lowest grid point whose false match rate is at or under `target`.

    `tail_desc` holds the highest impostor scores, highest first, and must reach
    past the cutoff; `total` counts every impostor pair. Returns None when the
    tail is too short to tell."""
    if total <= 0:
        return None
    allowed = math.floor(target * total * (1 + 1e-12))
    if allowed >= len(tail_desc):
        return None
    cutoff = grid_above(tail_desc[allowed])
    # Every score the tail holds above the cutoff is in its first `allowed` entries.
    assert count_at_or_above(tail_desc, cutoff) <= allowed
    return cutoff


def histogram_index(scores, bins):
    """Bin index of each score on [-1, 1] in `bins` equal bins."""
    index = np.floor((np.asarray(scores, dtype=np.float64) + 1.0) * (bins / 2.0)).astype(np.int64)
    return np.clip(index, 0, bins - 1)


def bootstrap_weights(groups, replicates, seed):
    """How many times each of `groups` identities is drawn, per replicate."""
    rng = np.random.default_rng(seed)
    if groups == 0:
        return np.zeros((replicates, 0))
    return rng.multinomial(groups, np.full(groups, 1.0 / groups), size=replicates).astype(np.float64)


def pair_sums(weights, ga, gb, values, chunk=64):
    """Per replicate, the weighted sum of `values` over pairs (ga, gb)."""
    ga, gb = np.asarray(ga, dtype=np.int64), np.asarray(gb, dtype=np.int64)
    values = np.asarray(values, dtype=np.float64)
    same = ga == gb
    out = np.zeros(weights.shape[0])
    if len(values) == 0:
        return out
    for start in range(0, weights.shape[0], chunk):
        w = weights[start : start + chunk]
        pw = np.where(same, w[:, ga], w[:, ga] * w[:, gb])
        out[start : start + chunk] = pw @ values
    return out


def impostor_totals(weights, images_per_group):
    """Per replicate, the number of different-person pairs: sum over a < b of w_a w_b c_a c_b."""
    c = np.asarray(images_per_group, dtype=np.float64)
    first = weights @ c
    return (first * first - (weights * weights) @ (c * c)) / 2.0


def percentile95(samples):
    samples = np.asarray(samples, dtype=np.float64)
    samples = samples[np.isfinite(samples)]
    if len(samples) == 0:
        return None
    return [float(np.percentile(samples, 2.5)), float(np.percentile(samples, 97.5))]


def bootstrap_rate(numerators, denominators):
    """95% percentile interval of numerator / denominator over replicates."""
    numerators, denominators = np.asarray(numerators, dtype=np.float64), np.asarray(denominators, dtype=np.float64)
    with np.errstate(divide="ignore", invalid="ignore"):
        rates = np.where(denominators > 0, numerators / np.where(denominators > 0, denominators, 1), np.nan)
    return percentile95(rates)


def aggregate_pairs(ga, gb, flags):
    """Collapse pairs to one row per (ga, gb), ga <= gb: pair count and flagged count."""
    ga, gb = np.asarray(ga, dtype=np.int64), np.asarray(gb, dtype=np.int64)
    lo, hi = np.minimum(ga, gb), np.maximum(ga, gb)
    if len(lo) == 0:
        empty = np.zeros(0, dtype=np.int64)
        return empty, empty, empty, empty
    span = int(hi.max()) + 1
    keys, inverse = np.unique(lo * span + hi, return_inverse=True)
    trials = np.bincount(inverse, minlength=len(keys))
    flagged = np.bincount(inverse, weights=np.asarray(flags, dtype=np.float64), minlength=len(keys))
    return keys // span, keys % span, trials, flagged


def grouped_pair_rate(weights, ga, gb, flags):
    """Identity-bootstrap 95% interval of the flagged share of pairs (ga, gb)."""
    a, b, trials, flagged = aggregate_pairs(ga, gb, flags)
    return bootstrap_rate(pair_sums(weights, a, b, flagged), pair_sums(weights, a, b, trials))


def bootstrap_cutoffs(weights, tail_ga, tail_gb, tail_desc, totals, target, chunk=32):
    """Per replicate, the lowest grid point whose weighted false match rate is at or under `target`."""
    tail_ga, tail_gb = np.asarray(tail_ga, dtype=np.int64), np.asarray(tail_gb, dtype=np.int64)
    tail_desc = np.asarray(tail_desc, dtype=np.float32)
    out = np.full(weights.shape[0], np.nan)
    for start in range(0, weights.shape[0], chunk):
        w = weights[start : start + chunk]
        cum = np.cumsum(w[:, tail_ga] * w[:, tail_gb], axis=1)
        allowed = target * totals[start : start + chunk]
        # The first tail entry that would push the count past what is allowed.
        over = cum > allowed[:, None] * (1 + 1e-12)
        has = over.any(axis=1)
        first = np.argmax(over, axis=1)
        for row in np.nonzero(has)[0]:
            out[start + row] = grid_above(tail_desc[first[row]])
    return out


def false_match_spread(ia, ib, ga, gb, top=10):
    """How concentrated the false matches are: images, people, and pairs of people behind them."""
    ia, ib, ga, gb = (np.asarray(x, dtype=np.int64) for x in (ia, ib, ga, gb))
    n = len(ia)
    if n == 0:
        return {"false_matches": 0, "distinct_images": 0, "distinct_identities": 0, "distinct_identity_pairs": 0, f"share_from_top_{top}_identity_pairs": None}
    lo, hi = np.minimum(ga, gb), np.maximum(ga, gb)
    _, counts = np.unique(lo * (int(hi.max()) + 1) + hi, return_counts=True)
    biggest = np.sort(counts)[::-1][:top]
    return {
        "false_matches": int(n),
        "distinct_images": int(len(np.unique(np.concatenate([ia, ib])))),
        "distinct_identities": int(len(np.unique(np.concatenate([ga, gb])))),
        "distinct_identity_pairs": int(len(counts)),
        f"share_from_top_{top}_identity_pairs": float(biggest.sum() / n),
    }


CHANCE_POSTERS = (50, 200, 1000)
CHANCE_FACES = (10, 100, 1000)


def chance_rows(fmr, fmr_upper, posters=CHANCE_POSTERS, faces=CHANCE_FACES):
    """Expected false candidates in one scan: FMR x posters x compared faces."""
    rows = []
    for n in posters:
        for m in faces:
            comparisons = n * m
            rows.append({
                "posters": n,
                "faces_compared": m,
                "comparisons": comparisons,
                "expected_false_candidates": fmr * comparisons,
                "expected_upper95": fmr_upper * comparisons,
                "chance_of_at_least_one": at_least_one(fmr, comparisons),
                "chance_of_at_least_one_upper95": at_least_one(fmr_upper, comparisons),
            })
    return rows


def at_least_one(p, n):
    if p <= 0:
        return 0.0
    if p >= 1:
        return 1.0
    return float(-math.expm1(n * math.log1p(-p)))


def z_for(tests):
    """The z that keeps `tests` two-sided checks at 95% together (Bonferroni)."""
    return NormalDist().inv_cdf(1 - 0.025 / max(1, tests))


def agrees(published_rate, sample_entry, z=Z95):
    """True when the published rate lies inside the sample's own widest interval at `z`."""
    if published_rate is None:
        return True
    if not sample_entry.get("trials"):
        return False
    lo, hi = interval(sample_entry)
    wlo, whi = wilson(sample_entry["count"], sample_entry["trials"], z)
    lo, hi = min(lo, wlo), max(hi, whi)
    return lo - 1e-12 <= published_rate <= hi + 1e-12
