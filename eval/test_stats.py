"""Unit tests for the counting in eval/. Synthetic numbers only, no photographs.

    python3 -m unittest discover -s eval -p 'test_*.py'
"""

import json
import math
import sys
import unittest
from itertools import combinations
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import real_faces as rf  # noqa: E402
import stats  # noqa: E402


def unit(v):
    v = np.asarray(v, dtype=np.float32)
    return v / np.linalg.norm(v, axis=-1, keepdims=True)


def clustered(people=40, photos=3, dim=16, spread=0.5, seed=1):
    """Photos of each person scattered around that person's direction."""
    rng = np.random.default_rng(seed)
    centers = unit(rng.normal(size=(people, dim)))
    emb, names = [], []
    for p in range(people):
        for _ in range(photos):
            emb.append(unit(centers[p] + spread * rng.normal(size=dim) / math.sqrt(dim)))
            names.append(f"person_{p:03d}")
    return np.array(emb, dtype=np.float32), names


def brute(emb, names):
    genuine, impostor = [], []
    for i, j in combinations(range(len(emb)), 2):
        s = np.float32(emb[i] @ emb[j])
        (genuine if names[i] == names[j] else impostor).append(s)
    return np.array(genuine, dtype=np.float32), np.array(impostor, dtype=np.float32)


class Intervals(unittest.TestCase):
    def test_wilson_matches_known_values(self):
        lo, hi = stats.wilson(5, 10)
        self.assertAlmostEqual(lo, 0.2366, places=4)
        self.assertAlmostEqual(hi, 0.7634, places=4)
        lo, hi = stats.wilson(0, 10)
        self.assertEqual(lo, 0.0)
        self.assertAlmostEqual(hi, 0.2775, places=4)
        self.assertEqual(stats.wilson(0, 0), [0.0, 1.0])

    def test_rate_and_interval(self):
        entry = stats.rate(3, 100, [0.0, 0.2])
        self.assertEqual(entry["rate"], 0.03)
        lo, hi = stats.interval(entry)
        self.assertEqual(lo, 0.0)
        self.assertEqual(hi, 0.2)
        self.assertTrue(stats.agrees(0.15, entry))
        self.assertFalse(stats.agrees(0.25, entry))
        self.assertTrue(stats.agrees(None, entry))
        self.assertFalse(stats.agrees(0.25, stats.rate(0, 0)))
        # Checked together with many other rates, the interval is wider.
        self.assertGreater(stats.z_for(40), stats.Z95)
        self.assertAlmostEqual(stats.z_for(1), stats.Z95, places=6)
        plain = stats.rate(3, 100)
        self.assertFalse(stats.agrees(0.15, plain))
        self.assertTrue(stats.agrees(0.15, plain, stats.z_for(1000)))

    def test_unit_weights_reproduce_the_point_rate(self):
        ga = np.array([0, 0, 1, 2, 2])
        gb = np.array([1, 2, 2, 2, 0])
        flags = np.array([1, 0, 1, 0, 0])
        w = np.ones((3, 3))
        a, b, trials, flagged = stats.aggregate_pairs(ga, gb, flags)
        rates = stats.pair_sums(w, a, b, flagged) / stats.pair_sums(w, a, b, trials)
        np.testing.assert_allclose(rates, 2 / 5)

    def test_pair_sums_equal_an_expanded_resample(self):
        # Person a drawn w_a times: a pair of different people counts w_a * w_b
        # times, one person's own pair w_a times.
        w = np.array([[2.0, 0.0, 1.0, 1.0]])
        ga, gb, values = [0, 0, 2, 0, 1], [2, 1, 3, 0, 3], [1.0, 5.0, 1.0, 1.0, 7.0]
        self.assertEqual(stats.pair_sums(w, ga, gb, values)[0], 2 * 1 * 1 + 2 * 0 * 5 + 1 * 1 * 1 + 2 * 1 + 0)

    def test_impostor_totals_match_brute_force(self):
        rng = np.random.default_rng(3)
        counts = rng.integers(1, 5, size=7)
        w = rng.integers(0, 3, size=(4, 7)).astype(float)
        want = [sum(w[r, a] * w[r, b] * counts[a] * counts[b] for a, b in combinations(range(7), 2)) for r in range(4)]
        np.testing.assert_allclose(stats.impostor_totals(w, counts), want)

    def test_identity_bootstrap_is_wider_when_one_person_makes_the_errors(self):
        # Fifty people, three thousand pairs, every error from pairs with person 0.
        rng = np.random.default_rng(5)
        ga = rng.integers(0, 50, size=3000)
        gb = (ga + rng.integers(1, 50, size=3000)) % 50
        flags = ((ga == 0) | (gb == 0)) & (rng.random(3000) < 0.5)
        k = int(flags.sum())
        lo, hi = stats.grouped_pair_rate(stats.bootstrap_weights(50, 400, 9), ga, gb, flags)
        wlo, whi = stats.wilson(k, 3000)
        self.assertLess(lo, wlo)
        self.assertGreater(hi, whi)
        # Without person 0 in a replicate there are no errors at all.
        self.assertEqual(lo, 0.0)

    def test_bootstrap_weights_are_draws_of_every_person(self):
        w = stats.bootstrap_weights(10, 20, 1)
        self.assertEqual(w.shape, (20, 10))
        np.testing.assert_array_equal(w.sum(axis=1), 10)
        np.testing.assert_array_equal(w, stats.bootstrap_weights(10, 20, 1))


class Cutoffs(unittest.TestCase):
    def test_lowest_grid_point_at_or_under_the_target(self):
        tail = np.array([0.95, 0.9, 0.8, 0.71234, 0.7, 0.6], dtype=np.float32)
        # 1000 pairs, at most 3 over: the fourth highest score must be under the cutoff.
        cutoff = stats.cutoff_from_tail(tail, 1000, 0.003)
        self.assertEqual(cutoff, 0.7124)
        self.assertLessEqual(stats.count_at_or_above(tail, cutoff), 3)
        self.assertGreater(stats.count_at_or_above(tail, cutoff - 0.0001), 3)
        self.assertIsNone(stats.cutoff_from_tail(tail, 1000, 0.01))
        self.assertIsNone(stats.cutoff_from_tail(tail, 0, 0.01))

    def test_a_score_on_the_grid_is_compared_in_float32(self):
        # float32(0.63) is a hair under 0.63. The scan compares in float32, so
        # a score equal to it is at the cutoff 0.63, and the cutoff must step up.
        tail = np.array([0.9, np.float32(0.63), 0.5], dtype=np.float32)
        cutoff = stats.cutoff_from_tail(tail, 1000, 0.001)
        self.assertEqual(cutoff, 0.6301)
        self.assertEqual(stats.count_at_or_above(tail, cutoff), 1)
        self.assertEqual(stats.count_at_or_above(tail, 0.63), 2)

    def test_bootstrap_cutoffs_with_unit_weights_are_the_cutoff(self):
        tail = np.array([0.9, 0.8, 0.7, 0.6], dtype=np.float32)
        w = np.ones((2, 3))
        ga, gb = [0, 0, 1, 1], [1, 2, 2, 2]
        totals = np.array([1000.0, 1000.0])
        np.testing.assert_allclose(stats.bootstrap_cutoffs(w, ga, gb, tail, totals, 0.002), [0.7001, 0.7001])

    def test_false_match_spread_counts_people_and_photos(self):
        spread = stats.false_match_spread([0, 0, 5], [1, 2, 6], [0, 0, 3], [1, 1, 4], top=1)
        self.assertEqual(spread["false_matches"], 3)
        self.assertEqual(spread["distinct_images"], 5)
        self.assertEqual(spread["distinct_identities"], 4)
        self.assertEqual(spread["distinct_identity_pairs"], 2)
        self.assertAlmostEqual(spread["share_from_top_1_identity_pairs"], 2 / 3)


class Chance(unittest.TestCase):
    def test_expected_false_candidates_are_fmr_times_posters_times_faces(self):
        rows = stats.chance_rows(1e-6, 4e-6)
        self.assertEqual(len(rows), 9)
        big = [r for r in rows if r["posters"] == 1000 and r["faces_compared"] == 1000][0]
        self.assertAlmostEqual(big["expected_false_candidates"], 1.0)
        self.assertAlmostEqual(big["expected_upper95"], 4.0)
        self.assertAlmostEqual(big["chance_of_at_least_one"], 1 - math.exp(-1), places=6)
        self.assertEqual(stats.at_least_one(0.0, 10), 0.0)


class AllPairs(unittest.TestCase):
    def test_counts_and_cutoff_match_brute_force(self):
        emb, names = clustered()
        genuine, impostor = brute(emb, names)
        section, cutoff = rf.lfw_statistics(emb, names, 0.5, replicates=30)
        self.assertEqual(section["impostor_pairs"], len(impostor))
        self.assertEqual(section["genuine_pairs"], len(genuine))
        at = section["at_manifest_threshold"]
        self.assertEqual(at["false_match"]["count"], int((impostor >= np.float32(0.5)).sum()))
        self.assertEqual(at["false_non_match"]["count"], int((genuine < np.float32(0.5)).sum()))
        self.assertEqual(at["false_non_match_one_pair_per_person"]["trials"], 40)
        allowed = math.floor(rf.CUTOFF_FMR * len(impostor))
        self.assertLessEqual(int((impostor >= np.float32(cutoff)).sum()), allowed)
        self.assertGreater(int((impostor >= np.float32(cutoff - 0.0001)).sum()), allowed)
        for op in section["operating_points"]:
            limit = math.floor(op["target_fmr"] * len(impostor))
            self.assertLessEqual(op["false_match"]["count"], limit)
        lo, hi = section["measured_cutoff_identity_bootstrap95"]
        self.assertLessEqual(lo, hi)
        table = {row["threshold"]: row for row in section["curve"]}
        self.assertEqual(table[0.5]["false_non_match"]["count"], at["false_non_match"]["count"])
        self.assertLessEqual(abs(table[0.5]["false_match"]["count"] - at["false_match"]["count"]), 2)

    def test_extra_thresholds_are_measured(self):
        emb, names = clustered(people=10)
        section, _ = rf.lfw_statistics(emb, names, 0.5, extra_thresholds=[0.3, 0.41234], replicates=10)
        self.assertEqual(sorted(section["at_thresholds"]), ["0.3000", "0.4123"])


class HeldOut(unittest.TestCase):
    def test_a_pair_without_a_face_is_a_miss_or_not_compared(self):
        pairs = [
            (True, 0.9, "a", "a"),
            (True, None, "b", "b"),
            (True, 0.2, "c", "c"),
            (False, 0.1, "a", "b"),
            (False, None, "b", "c"),
            (False, 0.7, "c", "d"),
        ]
        out = rf.pair_set_statistics(pairs, 0.6, replicates=20)
        self.assertEqual(out["false_non_match"]["count"], 2)
        self.assertEqual(out["false_non_match"]["trials"], 3)
        self.assertEqual(out["false_match"]["count"], 1)
        self.assertEqual(out["false_match"]["trials"], 2)
        self.assertEqual(out["different_person_pairs_not_compared"]["count"], 1)
        self.assertEqual(out["same_person_pairs_not_compared"], 1)

    def test_identity_from_an_lfw_style_name(self):
        self.assertEqual(rf.identity_of("raw/Aaron_Peirsol_0003.jpg"), "Aaron_Peirsol")
        self.assertEqual(rf.identity_of("train/Aaron_Peirsol_2.jpg"), "Aaron_Peirsol")
        self.assertEqual(rf.identity_of("train/Jane_Doe/0001.jpg"), "Jane_Doe")


def face(x, y, w, h, embedding=None, reject=None):
    return {"rect": [x, y, w, h], "det_short": min(w, h), "score": 0.9, "reject": reject, "embedding": embedding}


def report(tracks=(), candidates=(), status="complete"):
    """A result v2 report from an eval build: tracks carry every poster score."""
    return {
        "schema": "openworld.result.v2",
        "status": status,
        "provider": "CPUExecutionProvider",
        "detection_long_side": 640,
        "tracks": [{"track_id": i + 1, "status": st, "scores": [{"poster_id": k, "score": v} for k, v in sc.items()]}
                   for i, (st, sc) in enumerate(tracks)],
        "candidates": [{"track_id": t, "poster": {"id": p}, "score": s, "frame_index": f} for t, p, s, f in candidates],
    }


POSTERS = {"alice": unit([1, 0, 0, 0]), "bob": unit([0, 1, 0, 0])}


def scores(v):
    return {k: float(np.float32(v) @ p) for k, p in POSTERS.items()}


def still_case(gap=0.0):
    """Alice's probe found, a stranger with no card. `gap` moves the scan's scores off the library's."""
    alice = unit([0.9, 0.1, 0.4, 0.1])
    stranger = unit([0.1, 0.1, 0.2, 0.9])
    probes = [("alice", Path("a.png")), (None, Path("s.png"))]
    library = {
        "a.png": {"faces": [face(0, 0, 200, 200, alice.tolist())]},
        "s.png": {"faces": [face(0, 0, 200, 200, stranger.tolist()), face(300, 0, 50, 50, reject="why_small")]},
    }
    sa = {k: v + gap for k, v in scores(alice).items()}
    reports = [
        report([("candidate", sa)], [(1, "alice", sa["alice"], 0)]),
        report([("below_cutoff", scores(stranger)), ("not_compared", {})]),
    ]
    return probes, reports, library


def synthetic_scans(threshold, gap=0.0, stranger_card=False):
    probes, reports, library = still_case(gap)
    stills = rf.summarize_stills(probes, reports, library, POSTERS, threshold)
    seen = rf.LEAD_FRAMES + 3
    hit = report([("candidate", {"alice": 0.8, "bob": 0.1})], [(1, "alice", 0.8, seen)])
    stranger = report([("below_cutoff", {"alice": 0.2, "bob": 0.1})])
    if stranger_card:
        stranger = report([("candidate", {"alice": 0.9, "bob": 0.1})], [(1, "alice", 0.9, seen)])
    brief = report([("too_brief", {"alice": 0.8, "bob": 0.1})])
    video = rf.summarize_video([("alice", {"complete": hit, "measured": hit})], [stranger], [("alice", brief)], threshold)
    phone = rf.summarize_phone([
        (name, coverage, "alice", hit, 0.79)
        for name in rf.PHONE_FILES
        for coverage in (("complete", "measured") if name.startswith("portrait") else ("complete",))
    ])
    return {"posters": 2, "posters_left_out": [], "stills": stills, "video": video, "phone_files": phone}


def synthetic_curve(gap=0.0, stranger_card=False):
    emb, names = clustered(people=30)
    first, cutoff = rf.lfw_statistics(emb, names, 0.9, replicates=20)
    threshold = round(cutoff + 0.01, 2)
    extra = [op["threshold"] for op in first["operating_points"] if op.get("threshold") is not None]
    lfw, cutoff = rf.lfw_statistics(emb, names, threshold, extra, replicates=20)
    rows = {"p1": {"width": 100, "height": 100, "faces": [face(30, 30, 40, 40, [1.0])]},
            "p2": {"width": 100, "height": 100, "faces": [face(30, 30, 40, 40, reject="why_pose")]},
            "p3": {"path": "p3", "error": "decode"}}
    quality = {"lfw_enlarged_2x": rf.gate_summary(rows, rows), "lfw_native_size": rf.gate_summary(rows, rows)}
    pairs = [(True, 0.9, "a", "a"), (True, 0.3, "b", "b"), (False, 0.1, "a", "b"), (False, None, "b", "c")]
    held = {name: {"at_manifest_threshold": rf.pair_set_statistics(pairs, threshold, replicates=10)} for name in rf.HELD_OUT}
    truth = {"img": [{"rect": (0, 0, 200, 200), "occlusion": 0, "pose": 0, "blur": 0},
                     {"rect": (300, 0, 80, 80), "occlusion": 1, "pose": 0, "blur": 0},
                     {"rect": (500, 0, 150, 150), "occlusion": 2, "pose": 1, "blur": 0}]}
    wrows = {"img": {"width": 1000, "height": 1000, "detection_long_side": 1000,
                     "faces": [face(5, 5, 195, 195, [1.0]), face(305, 2, 78, 78, reject="why_small"), face(900, 900, 50, 50)]}}
    wider, quality["wider_face_val"] = rf.wider_statistics(truth, wrows)
    scans = synthetic_scans(threshold, gap, stranger_card)
    manifest = {"version": "0.2.0", "models": {"detector": "YuNet"}, "files": [{"role": "embedder", "sha256": "0" * 64}]}
    curve = rf.build_curve("fast", manifest, threshold, cutoff, None, lfw, held, scans, wider, quality, {"bench": {}})
    # The file must survive JSON: no numpy numbers left in it.
    return json.loads(json.dumps(curve))


class Scans(unittest.TestCase):
    def test_stills_count_found_strangers_and_the_cosine_gap(self):
        probes, reports, library = still_case()
        out = rf.summarize_stills(probes, reports, library, POSTERS, 0.6)
        self.assertEqual(out["enrolled_found"]["count"], 1)
        self.assertEqual(out["stranger_any_candidate"]["count"], 0)
        self.assertEqual(out["face_count_differs_from_library"], 0)
        self.assertLess(out["largest_cosine_gap_vs_library"], 1e-6)
        self.assertEqual(out["comparisons_with_other_posters"]["trials"], 3)
        off = rf.summarize_stills(*still_case(gap=0.001)[:2], still_case()[2], POSTERS, 0.6)
        self.assertGreater(off["largest_cosine_gap_vs_library"], rf.COSINE_TOLERANCE)

    def test_a_scan_without_scores_is_refused(self):
        with self.assertRaises(SystemExit):
            rf.require_scores({"tracks": [{"status": "below_cutoff"}]})
        rf.require_scores({"tracks": [{"status": "not_compared"}]})

    def test_video_outcomes(self):
        out = rf.summarize_video(
            [("alice", {"complete": report([("candidate", {"alice": 0.8})], [(1, "alice", 0.8, rf.LEAD_FRAMES)]),
                        "measured": report([("below_cutoff", {"alice": 0.5})])})],
            [report([("below_cutoff", {"alice": 0.2})]), report(status="incomplete")],
            [("alice", report([("too_brief", {"alice": 0.8})])), ("alice", report([("below_cutoff", {"alice": 0.4})]))],
            0.6,
        )
        self.assertEqual(out["complete"]["found"]["count"], 1)
        self.assertEqual(out["complete"]["card_on_the_shown_second"]["count"], 1)
        self.assertEqual(out["measured"]["found"]["count"], 0)
        self.assertEqual(out["stranger_clips"]["any_candidate"]["trials"], 1)
        self.assertEqual(out["stranger_clips"]["not_complete"], 1)
        brief = out["two_frame_appearance"]
        self.assertEqual(brief["too_brief"]["count"], 1)
        self.assertEqual(brief["held_back_when_over_the_cutoff"]["trials"], 1)
        self.assertEqual(brief["outcomes"], {"below_cutoff": 1, "too_brief": 1})

    def test_wider_bands_and_occlusion(self):
        curve = synthetic_curve()
        wider = curve["wider_face_val"]
        self.assertEqual(wider["faces_112px_and_up"]["all"]["trials"], 2)
        self.assertEqual(wider["faces_112px_and_up"]["all"]["count"], 1)
        self.assertEqual(wider["faces_112px_and_up"]["occluded"]["count"], 0)
        self.assertEqual(wider["faces_64_to_112px"]["all"]["count"], 1)
        self.assertEqual(wider["detections_off_every_labeled_face"], 1)
        self.assertEqual(curve["quality"]["wider_face_val"]["outcome"], {"compared": 1})


class Checks(unittest.TestCase):
    def test_a_run_agrees_with_itself(self):
        curve = synthetic_curve()
        self.assertEqual(rf.invariants(curve), [])
        self.assertEqual(rf.full_run_rules(curve), [])
        self.assertEqual(rf.check(curve, curve), [])
        self.assertEqual(curve["chance"]["fmr_at_cutoff"], curve["lfw"]["at_manifest_threshold"]["false_match"]["rate"])
        self.assertEqual(curve["chance"]["source"], "LFW all pairs, faces enlarged 2x")

    def test_a_rate_at_an_operating_point_is_checked(self):
        curve = synthetic_curve()
        published = json.loads(json.dumps(curve))
        for op in published["lfw"]["operating_points"]:
            if op["target_fmr"] == 1e-4:
                op["false_match"]["rate"] = 0.5
        self.assertTrue(any("published FMR 0.0001" in f for f in rf.check(curve, published)))

    def test_a_published_rate_outside_the_sample_interval_fails(self):
        curve = synthetic_curve()
        published = json.loads(json.dumps(curve))
        published["lfw"]["at_manifest_threshold"]["false_non_match_one_pair_per_person"]["rate"] = 0.99
        published["scans"]["stills"]["enrolled_found"]["rate"] = 0.0
        failures = rf.check(curve, published)
        self.assertTrue(any("LFW false_non_match_one_pair_per_person at the manifest threshold" in f for f in failures))
        self.assertTrue(any("stills enrolled_found" in f for f in failures))

    def test_the_lfw_false_match_rate_is_checked(self):
        curve = synthetic_curve()
        published = json.loads(json.dumps(curve))
        published["lfw"]["at_manifest_threshold"]["false_match"]["rate"] = 0.5
        published["chance"]["fmr_at_cutoff"] = 0.5
        self.assertTrue(any("LFW false_match at the manifest threshold" in f for f in rf.check(curve, published)))

    def test_published_curve_must_keep_real_posters_off_and_be_consistent(self):
        curve = synthetic_curve()
        published = json.loads(json.dumps(curve))
        published["real_posters_allowed"] = True
        published["chance"]["fmr_at_cutoff"] = 0.123
        failures = rf.check(curve, published)
        self.assertTrue(any("real_posters_allowed" in f for f in failures))
        self.assertTrue(any("chance.fmr_at_cutoff" in f for f in failures))

    def test_strangers_and_score_gaps_fail_any_run(self):
        self.assertTrue(any("stranger clips" in f for f in rf.invariants(synthetic_curve(stranger_card=True))))
        self.assertTrue(any("library's cosines" in f for f in rf.invariants(synthetic_curve(gap=0.001))))

    def test_a_manifest_under_the_cutoff_fails_a_full_run(self):
        curve = synthetic_curve()
        curve["threshold"] = curve["measured_cutoff"] - 0.01
        self.assertTrue(rf.full_run_rules(curve))


class Clips(unittest.TestCase):
    def test_bilinear_resize_keeps_a_flat_image_flat(self):
        flat = np.full((10, 20, 3), 77, dtype=np.uint8)
        out = rf.resize_bilinear(flat, 33, 7)
        self.assertEqual(out.shape, (7, 33, 3))
        self.assertTrue((out == 77).all())

    def test_a_clip_shows_the_face_only_for_its_frames(self):
        face_img = np.full((50, 50, 3), 250, dtype=np.uint8)
        frames = list(rf.clip_frames(face_img, (200, 120), 40, (20, 30), 4, lead=2, tail=3))
        self.assertEqual(len(frames), 9)
        shown = [bool((f == 250).all(axis=2).any()) for f in frames]
        self.assertEqual(shown, [False, False, True, True, True, True, False, False, False])
        sizes = [rf.motion(k, 30, 500, (0, 0))[0] for k in range(30)]
        self.assertEqual(sizes, sorted(sizes))
        self.assertAlmostEqual(sizes[0], 0.9)
        self.assertAlmostEqual(sizes[-1], 1.0)

    def test_orientation_tag_goes_after_the_jfif_segment(self):
        jpeg = b"\xff\xd8\xff\xe0\x00\x10" + b"J" * 14 + b"\xff\xdbrest"
        tagged = rf.with_orientation(jpeg, 6)
        self.assertEqual(tagged[:20], jpeg[:20])
        self.assertEqual(tagged[20:22], b"\xff\xe1")
        self.assertIn(b"Exif\0\0II*\0", tagged)
        self.assertTrue(tagged.endswith(b"\xff\xdbrest"))


if __name__ == "__main__":
    unittest.main()
