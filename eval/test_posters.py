"""Unit tests for the poster-photo measurement and its gate. Synthetic items and vectors only.

    python3 -m unittest discover -s eval -p 'test_*.py'
"""

import json
import sys
import tempfile
import unittest
from itertools import combinations
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import posters  # noqa: E402
import real_faces as rf  # noqa: E402
import stats  # noqa: E402
from test_stats import synthetic_curve  # noqa: E402


def unit(v):
    v = np.asarray(v, dtype=np.float32)
    return v / np.linalg.norm(v, axis=-1, keepdims=True)


class Classifier(unittest.TestCase):
    def test_the_shared_cases(self):
        # The core runs the same file in update.rs (classify_matches_the_shared_cases).
        cases = json.loads((HERE / "fbi_classify_cases.json").read_text())["cases"]
        self.assertGreater(len(cases), 20)
        for case in cases:
            with self.subTest(case["name"]):
                poster, why = posters.classify(case["item"])
                self.assertEqual(poster, case["want"])
                self.assertEqual(why, case["why"])
                self.assertEqual(posters.classify_item(case["item"]), case["want"])

    def test_rust_trim_and_ascii_case(self):
        self.assertEqual("\x1fA ".strip(posters.RUST_WHITESPACE), "\x1fA")
        self.assertEqual(" A ".strip(posters.RUST_WHITESPACE), "A")
        self.assertEqual("straße".translate(posters.ASCII_UPPER), "STRAßE")

    def test_list_counts_carry_no_person(self):
        items = [case["item"] for case in json.loads((HERE / "fbi_classify_cases.json").read_text())["cases"]]
        kept, summary = posters.summarize_list(items, len(items))
        self.assertTrue(summary["complete"])
        self.assertEqual(sum(summary["kept"].values()), len(kept))
        self.assertEqual(sum(summary["skipped"].values()) + len(kept), len(items))
        self.assertIn("default", summary["poster_classification_skipped"])
        text = json.dumps(summary)
        for poster in kept:
            self.assertNotIn(poster["title"], text)
            self.assertNotIn(poster["fbi_url"], text)


class Fetching(unittest.TestCase):
    def test_the_list_is_paged_to_its_total(self):
        people = [{"uid": f"u{i}", "title": f"P {i}"} for i in range(120)]
        asked, slept = [], []

        def get(url):
            asked.append(url)
            page = int(url.split("page=")[1].split("&")[0])
            size = int(url.split("pageSize=")[1])
            # The list shifts by one between pages: an item comes back twice.
            start = max(0, (page - 1) * size - (1 if page > 1 else 0))
            return json.dumps({"total": 120, "items": people[start : start + size], "page": page}).encode()

        items, total = posters.fetch_list(get, sleep=slept.append)
        self.assertEqual(total, 120)
        self.assertEqual([i["uid"] for i in items], [p["uid"] for p in people])
        self.assertTrue(asked[0].startswith(posters.FBI_LIST + "?page=1&pageSize="))
        self.assertEqual(len(slept), len(asked) - 1)

    def test_an_empty_page_stops_short_of_the_total(self):
        def get(url):
            page = int(url.split("page=")[1].split("&")[0])
            return json.dumps({"total": 10, "items": [{"uid": "a"}] if page == 1 else []}).encode()

        items, total = posters.fetch_list(get, sleep=lambda _: None)
        self.assertEqual((len(items), total), (1, 10))
        _, summary = posters.summarize_list(items, total)
        self.assertFalse(summary["complete"])

    def test_photos_come_only_from_fbi_gov(self):
        for url in ("https://www.fbi.gov/wanted/x/@@images/image", "https://fbi.gov/a.jpg", "https://API.FBI.GOV/x"):
            self.assertTrue(posters.fbi_host(url), url)
        for url in ("http://www.fbi.gov/x", "https://www.fbi.gov.example.com/x", "https://example.com/fbi.gov",
                    "https://notfbi.gov/x", None, "", "https://[bad"):
            self.assertFalse(posters.fbi_host(url), url)

    def test_downloads_first_photos_first_up_to_the_limit(self):
        kept = [
            {"id": "a", "images": [{"url": "https://www.fbi.gov/a1"}, {"url": "https://www.fbi.gov/a2"}]},
            {"id": "b", "images": [{"url": "https://example.com/b1"}, {"url": "https://www.fbi.gov/b2"}]},
            {"id": "c", "images": [{"url": None}, {"url": "https://www.fbi.gov/c2"}, {"url": "https://www.fbi.gov/c3"}]},
        ]
        fetched = []

        def get(url):
            fetched.append(url)
            if url.endswith("b2"):
                raise OSError("404")
            return b"\xff\xd8\xff" + url.encode()

        with tempfile.TemporaryDirectory() as tmp:
            posters.download_photos(kept, Path(tmp), get, workers=1, limit=3, sleep=lambda _: None)
            self.assertEqual(fetched, ["https://www.fbi.gov/a1", "https://www.fbi.gov/a2", "https://www.fbi.gov/b2"])
            a1 = kept[0]["images"][0]
            self.assertTrue(a1["path"].endswith(".jpg") and Path(a1["path"]).is_file())
            self.assertEqual(Path(a1["path"]).stem, a1["sha256"])
        self.assertEqual(kept[1]["images"][0]["outcome"], "off_fbi_gov")
        self.assertEqual(kept[1]["images"][1]["outcome"], "download_failed")
        self.assertEqual(kept[2]["images"][0]["outcome"], "no_url")
        self.assertEqual([im["outcome"] for im in kept[2]["images"][1:]], ["over_the_photo_limit"] * 2)

    def test_demographics_are_raw_or_unknown(self):
        self.assertEqual(posters.demographics({"sex": "Female", "race": "", "race_raw": " White (Hispanic) "}),
                         {"sex": "Female", "race": "unknown", "race_raw": "White (Hispanic)"})
        self.assertEqual(posters.demographics({}), {f: "unknown" for f in posters.DEMOGRAPHIC_FIELDS})
        refs, extra = posters.photo_refs({"images": [{"original": f"https://www.fbi.gov/{i}", "caption": "Age-progressed to 40" if i else "1999"}
                                                     for i in range(7)]})
        self.assertEqual((len(refs), extra), (posters.PHOTOS_PER_POSTER, 7 - posters.PHOTOS_PER_POSTER))
        self.assertEqual([r["age_progressed"] for r in refs[:2]], [False, True])

    def test_a_sample_is_the_same_every_time(self):
        kept = [{"id": f"{i:03d}"} for i in range(100)]
        a = posters.sample_posters(list(reversed(kept)), 10)
        self.assertEqual([p["id"] for p in a], [f"{i:03d}" for i in range(0, 100, 10)])
        self.assertEqual(len(posters.sample_posters(kept, None)), 100)


def photo(vector, sha, outcome="compared", aged=False):
    out = {"url": "https://www.fbi.gov/x", "outcome": outcome, "sha256": sha, "age_progressed": aged}
    if outcome == "compared":
        out["embedding"] = unit(vector).tolist()
    return out


def poster(uid, title, photos, sex="Male", race="white", race_raw="White"):
    return {"id": uid, "class": "wanted", "title": title, "fbi_url": "https://www.fbi.gov/wanted",
            "groups": {"sex": sex, "race": race, "race_raw": race_raw}, "images": photos}


def synthetic_posters(seed=4, people=12, dim=8):
    rng = np.random.default_rng(seed)
    centers = unit(rng.normal(size=(people, dim)))
    out = []
    for p in range(people):
        sex = "Female" if p % 3 == 0 else "Male"
        race = ["white", "black", ""][p % 3] or "unknown"
        shots = [photo(centers[p] + 0.3 * rng.normal(size=dim), f"{p}-{k}", aged=(k == 2)) for k in range(1 + p % 3)]
        out.append(poster(f"u{p}", f"PERSON {p}", shots, sex=sex, race=race, race_raw=race))
    return out


class Measurement(unittest.TestCase):
    def brute(self, items, lfw, t):
        """Impostor counts by brute force, with the same merging and grouping rules."""
        compared = [(p, [im for im in p["images"] if im["outcome"] == "compared"]) for p in items]
        compared = [(p, ims) for p, ims in compared if ims]
        cluster = posters.person_clusters([p for p, _ in compared])
        flat = [(cluster[i], p["groups"], np.float32(np.asarray(im["embedding"], dtype=np.float32)))
                for i, (p, ims) in enumerate(compared) for im in ims]
        pp = [(a, b) for a, b in combinations(flat, 2) if a[0] != b[0]]
        pp_hits = sum(bool(np.float32(a[2] @ b[2]) >= np.float32(t)) for a, b in pp)
        pl_hits = sum(bool(np.float32(a[2] @ v) >= np.float32(t)) for a in flat for v in lfw)
        return len(pp), pp_hits, len(flat) * len(lfw), pl_hits, flat, pp

    def test_counts_match_brute_force(self):
        items = synthetic_posters()
        rng = np.random.default_rng(9)
        lfw = unit(rng.normal(size=(30, 8)))
        names = [f"lfw{i // 2}" for i in range(30)]
        t = 0.3
        out = posters.statistics(items, lfw, names, t, replicates=50)
        pp_n, pp_k, pl_n, pl_k, flat, pp = self.brute(items, lfw, t)
        self.assertEqual(out["impostor"]["poster_x_poster"]["false_match"]["trials"], pp_n)
        self.assertEqual(out["impostor"]["poster_x_poster"]["false_match"]["count"], pp_k)
        self.assertEqual(out["impostor"]["poster_x_lfw"]["false_match"]["trials"], pl_n)
        self.assertEqual(out["impostor"]["poster_x_lfw"]["false_match"]["count"], pl_k)
        self.assertGreater(pp_k, 0)
        self.assertGreater(pl_k, 0)
        self.assertIn("identity_bootstrap95", out["impostor"]["poster_x_lfw"]["false_match"])
        # Groups: both posters in the group for poster x poster; the poster in the group for poster x LFW.
        for field in posters.DEMOGRAPHIC_FIELDS:
            groups = out["impostor"]["poster_x_poster"]["by_group"][field]
            for value, entry in groups.items():
                inside = [(a, b) for a, b in pp if a[1][field] == value and b[1][field] == value]
                self.assertEqual(entry["trials"], len(inside), (field, value))
                self.assertEqual(entry["count"], sum(bool(np.float32(a[2] @ b[2]) >= np.float32(t)) for a, b in inside))
            lfw_groups = out["impostor"]["poster_x_lfw"]["by_group"][field]
            self.assertEqual(sum(e["trials"] for e in lfw_groups.values()), pl_n)
            self.assertEqual(sum(e["count"] for e in lfw_groups.values()), pl_k)
        self.assertEqual(sorted(out["impostor"]["poster_x_lfw"]["by_group"]["race"]), ["black", "unknown", "white"])
        self.assertEqual(sorted(out["impostor"]["poster_x_lfw"]["by_group"]["sex"]), ["Female", "Male"])
        self.assertEqual(out["chance"]["posters"], 12)
        self.assertEqual(len(out["chance"]["per_scan"]), 3)

    def test_the_same_person_on_two_posters_is_not_an_impostor(self):
        a, b = unit([1, 0, 0]), unit([0, 1, 0])
        items = [
            poster("u1", "Jane Example", [photo(a, "s1")]),
            poster("u2", "JANE  EXAMPLE", [photo(a, "s2")]),  # the same name
            poster("u3", "Other Person", [photo(a, "s1")]),  # the same photo file
            poster("u4", "Fourth Person", [photo(b, "s4")]),
        ]
        out = posters.statistics(items, np.zeros((0, 3)), [], 0.5, replicates=10)
        self.assertEqual(out["people"]["people_after_merging"], 2)
        fm = out["impostor"]["poster_x_poster"]["false_match"]
        self.assertEqual((fm["count"], fm["trials"]), (0, 3))
        self.assertEqual(out["impostor"]["poster_x_lfw"]["false_match"]["trials"], 0)

    def test_genuine_pairs_and_posters_without_a_face(self):
        a = unit([1, 0, 0, 0])
        near = unit([1, 0.8, 0, 0])
        far = unit([0, 0, 1, 0])
        items = [
            poster("g1", "ONE PERSON", [photo(a, "1"), photo(near, "2"), photo(far, "3", aged=True), photo(a, "1")]),
            poster("g2", "AMINA AND BELEL EXAMPLE", [photo(a, "4"), photo(far, "5")]),
            poster("g3", "SAME TWICE", [photo(a, "6"), photo(a + 1e-4, "7")]),
            poster("n1", "NO FACE", [photo(a, "8", outcome="why_small"), photo(a, "9", outcome="why_small"), photo(a, "10", outcome="no_face")]),
            poster("n2", "NO PHOTO", []),
            poster("n3", "NOT FETCHED", [{"url": None, "outcome": "download_failed"}]),
        ]
        out = posters.statistics(items, np.zeros((0, 4)), [], 0.6, replicates=10)
        gen = out["genuine"]
        self.assertEqual(gen["posters_with_two_or_more_compared_photos"], 2)
        self.assertEqual(gen["posters_left_out_naming_more_than_one_person"], 1)
        self.assertEqual(gen["pairs_left_out_as_the_same_picture"], 1)
        # g1: a-near passes (0.78), a-far and near-far miss.
        self.assertEqual((gen["false_non_match"]["count"], gen["false_non_match"]["trials"]), (2, 3))
        self.assertEqual((gen["false_non_match_one_pair_per_poster"]["count"], gen["false_non_match_one_pair_per_poster"]["trials"]), (0, 1))
        self.assertEqual(gen["false_non_match_with_an_age_progressed_photo"]["trials"], 2)
        self.assertEqual(gen["false_non_match_without_one"]["trials"], 1)
        self.assertEqual(out["photos"]["same_file_twice_on_a_poster"], 1)
        self.assertEqual(out["posters_without_a_compared_face"],
                         {"count": 3, "reasons": {"download_failed": 1, "no_photo_listed": 1, "why_small": 1}})
        self.assertEqual(out["posters_with_a_compared_face"]["count"], 3)

    def test_nothing_compared(self):
        out = posters.statistics([poster("n", "NO FACE", [photo([1, 0], "x", outcome="no_face")])], np.zeros((0, 2)), [], 0.6, replicates=5)
        self.assertEqual(out["impostor"]["poster_x_poster"]["false_match"]["trials"], 0)
        self.assertEqual(out["genuine"]["false_non_match"]["trials"], 0)
        json.dumps(out)

    def test_the_section_survives_json(self):
        items = synthetic_posters()
        out = posters.statistics(items, unit(np.random.default_rng(2).normal(size=(6, 8))), ["a", "a", "b", "b", "c", "c"], 0.3, replicates=10)
        self.assertEqual(json.loads(json.dumps(out))["posters"], 12)


def section(pp=(0, 5_000_000), pl=(0, 50_000_000), groups=None, complete=True, sample=None):
    """A measured poster-photo section with only what the gate reads."""
    groups = groups or {}

    def part(k, n, kind):
        by_group = {field: {value: stats.rate(*counts) for value, counts in values.items()}
                    for field, values in groups.get(kind, {}).items()}
        return {"false_match": stats.rate(k, n, [0.0, (k + 1) / max(n, 1)]), "by_group": by_group}

    return {
        "measured": True,
        "sample": sample,
        "list": {"total": 100, "items_read": 100 if complete else 60, "complete": complete},
        "impostor": {"poster_x_poster": part(*pp, "poster_x_poster"), "poster_x_lfw": part(*pl, "poster_x_lfw")},
        "posters_with_a_compared_face": stats.rate(90, 100),
        "genuine": {"false_non_match": stats.rate(10, 100), "false_non_match_one_pair_per_poster": stats.rate(5, 50)},
    }


class Gate(unittest.TestCase):
    def test_a_full_clean_run_turns_posters_on(self):
        gate = rf.poster_gate(section(), True)
        self.assertTrue(gate["allowed"], gate["failures"])
        self.assertEqual(gate["gate_fmr"], rf.GATE_FMR)

    def test_the_proposed_constants(self):
        self.assertEqual((rf.GATE_FMR, rf.GATE_GROUP_FMR, rf.GATE_MIN_GROUP_COMPARISONS), (2e-6, 1e-5, 100_000))

    def test_an_overall_upper_bound_over_the_gate_blocks(self):
        # 5 in 5 million: the rate is 1e-6, but its upper bound is over 2e-6.
        gate = rf.poster_gate(section(pp=(5, 5_000_000)), True)
        self.assertFalse(gate["allowed"])
        self.assertTrue(any(f.startswith("poster_x_poster:") for f in gate["failures"]))
        # Too few comparisons to show the rate, even with no false match.
        self.assertFalse(rf.poster_gate(section(pp=(0, 1_000_000)), True)["allowed"])
        self.assertFalse(rf.poster_gate(section(pl=(0, 0)), True)["allowed"])

    def test_a_group_over_the_gate_blocks_and_a_small_one_is_listed(self):
        groups = {"poster_x_lfw": {"race": {"black": (0, 2_000_000), "asian": (2, 50_000)},
                                   "sex": {"Female": (40, 1_000_000)}}}
        gate = rf.poster_gate(section(groups=groups), True)
        self.assertFalse(gate["allowed"])
        self.assertEqual(len(gate["failures"]), 1)
        self.assertIn("poster_x_lfw sex=Female", gate["failures"][0])
        self.assertEqual(gate["not_enough_data"], ["poster_x_lfw race=asian: 50000 comparisons"])
        # Without the failing group, the small one does not block.
        groups["poster_x_lfw"]["sex"] = {"Female": (1, 1_000_000)}
        gate = rf.poster_gate(section(groups=groups), True)
        self.assertTrue(gate["allowed"], gate["failures"])
        self.assertEqual(len(gate["not_enough_data"]), 1)

    def test_a_group_at_the_minimum_with_no_false_match_still_blocks(self):
        # 0 of 100,000 has a Wilson upper bound near 3.8e-5, over 1e-5. See the comment on GATE_FMR.
        gate = rf.poster_gate(section(groups={"poster_x_poster": {"sex": {"Male": (0, 100_000)}}}), True)
        self.assertFalse(gate["allowed"])
        self.assertEqual(gate["not_enough_data"], [])

    def test_samples_and_missing_data_never_turn_posters_on(self):
        self.assertFalse(rf.poster_gate(section(), False)["allowed"])
        self.assertFalse(rf.poster_gate(section(sample={"posters": 100}), True)["allowed"])
        self.assertFalse(rf.poster_gate(section(complete=False), True)["allowed"])
        self.assertFalse(rf.poster_gate(None, True)["allowed"])
        self.assertFalse(rf.poster_gate({"measured": False, "note": "x"}, True)["allowed"])


def with_posters(curve, poster_domain):
    return json.loads(json.dumps(rf.build_curve(
        curve["bundle"], {"version": curve["bundle_version"], "models": curve["models"], "files": []}, curve["threshold"],
        curve["measured_cutoff"], curve["sample"], curve["lfw"], curve["held_out_pairs"], curve["scans"],
        curve["wider_face_val"], curve["quality"], curve["hardware"], poster_domain)))


def measured_section(n_posters=12):
    items = synthetic_posters(people=n_posters)
    lfw = unit(np.random.default_rng(2).normal(size=(6, 8)))
    out = posters.statistics(items, lfw, ["a", "a", "b", "b", "c", "c"], 0.95, replicates=10)
    return {"measured": True, "sample": None, "list": {"total": n_posters, "items_read": n_posters, "complete": True}, **out}


class CurveAndCheck(unittest.TestCase):
    def test_without_a_poster_section_posters_stay_off(self):
        curve = synthetic_curve()
        self.assertFalse(curve["real_posters_allowed"])
        self.assertEqual(curve["picker_line"], rf.PICKER_LINE)
        self.assertFalse(curve["poster_domain"]["measured"])
        self.assertIn(rf.NOT_OFF, curve["not"])
        notes = []
        self.assertEqual(rf.check(curve, curve, notes), [])
        self.assertEqual(len(notes), 1)
        self.assertIn("no poster-photo measurement", notes[0])

    def test_a_published_curve_without_the_section_skips_its_rates(self):
        curve = with_posters(synthetic_curve(), measured_section())
        published = synthetic_curve()
        published.pop("poster_domain")
        notes = []
        self.assertEqual(rf.check(curve, published, notes), [])
        self.assertIn("The published curve has no poster-photo measurement", notes[0])

    def test_the_gate_sets_the_switch_and_the_line(self):
        small = with_posters(synthetic_curve(), measured_section())
        # Twelve posters are far too few comparisons: posters stay off.
        self.assertFalse(small["real_posters_allowed"])
        self.assertEqual(small["picker_line"], rf.PICKER_LINE)
        on = with_posters(synthetic_curve(), section())
        self.assertTrue(on["real_posters_allowed"], on["poster_domain"]["gate"]["failures"])
        self.assertEqual(on["picker_line"], rf.PICKER_LINE_ON)
        self.assertIn(rf.NOT_ON, on["not"])
        self.assertNotIn(rf.NOT_OFF, on["not"])

    def test_a_published_switch_must_follow_its_own_section(self):
        curve = with_posters(synthetic_curve(), measured_section())
        self.assertEqual(rf.check(curve, curve), [])
        forged = json.loads(json.dumps(curve))
        forged["real_posters_allowed"] = True
        forged["picker_line"] = rf.PICKER_LINE_ON
        failures = rf.check(curve, forged)
        self.assertTrue(any("real_posters_allowed" in f for f in failures))
        self.assertTrue(any("picker_line" in f for f in failures))
        # A sampled published curve never allows real posters.
        on = with_posters(synthetic_curve(), section())
        on["sample"] = {"people": 300}
        self.assertTrue(any("real_posters_allowed" in f for f in rf.check(on, on)))

    def test_poster_rates_are_checked_against_the_published_ones(self):
        curve = with_posters(synthetic_curve(), measured_section())
        rates, note = rf.poster_rates(curve, curve)
        self.assertIsNone(note)
        names = [name for name, _, _ in rates]
        self.assertIn("posters poster_x_poster false_match", names)
        self.assertTrue(any("race=black" in n for n in names))
        published = json.loads(json.dumps(curve))
        published["poster_domain"]["impostor"]["poster_x_lfw"]["false_match"]["rate"] = 0.9
        failures = rf.check(curve, published)
        self.assertTrue(any("posters poster_x_lfw false_match" in f for f in failures))

    def test_measure_posters_end_to_end_without_a_network(self):
        rng = np.random.default_rng(11)
        centers = unit(rng.normal(size=(4, 8)))
        items = [
            {"uid": f"u{i}", "title": f"PERSON {i}", "poster_classification": "Wanted", "url": f"https://www.fbi.gov/wanted/{i}",
             "sex": "Male" if i % 2 else "Female", "race": "white",
             "images": [{"original": f"https://www.fbi.gov/wanted/{i}/@@images/image/{k}"} for k in range(2)]}
            for i in range(4)
        ] + [{"uid": "x", "title": "CHILD", "poster_classification": "ECAP"},
             {"uid": "y", "title": "OFF HOST", "poster_classification": "Missing", "url": "https://www.fbi.gov/wanted/y",
              "images": [{"original": "https://example.com/y.jpg"}]}]

        def fake_get(url, **_):
            if url.startswith(posters.FBI_LIST):
                return json.dumps({"total": len(items), "items": items}).encode()
            self.assertTrue(posters.fbi_host(url))
            return b"\xff\xd8\xff" + url.encode()

        class FakeVectors:
            def rows(self, paths, enlarge=1):
                self_test.assertEqual(enlarge, 1)
                out = {}
                for path in paths:
                    body = Path(path).read_bytes()[3:].decode()
                    person, shot = int(body.split("/")[-4]), int(body.rsplit("/", 1)[1])
                    vec = unit(centers[person] + 0.2 * np.sin(np.arange(8) + shot)).tolist()
                    # The second photo of person 3 has a face too small to compare.
                    faces = [{"rect": [10, 10, 120, 120], "embedding": None if (person, shot) == (3, 1) else vec,
                              "reject": "why_small" if (person, shot) == (3, 1) else None}]
                    out[path] = {"path": path, "width": 140, "height": 140, "faces": faces}
                return out

        self_test = self
        args = type("Args", (), {"no_posters": False, "posters": None})()
        saved = rf.get_bytes
        rf.get_bytes = fake_get
        original_pause = posters.PAUSE_SECONDS
        posters.PAUSE_SECONDS = 0.0
        try:
            with tempfile.TemporaryDirectory() as tmp:
                section = rf.measure_posters(args, FakeVectors(), Path(tmp), 0.63,
                                             (unit(rng.normal(size=(5, 8))), ["a", "a", "b", "c", "c"]))
        finally:
            rf.get_bytes = saved
            posters.PAUSE_SECONDS = original_pause
        self.assertTrue(section["measured"])
        self.assertTrue(section["list"]["complete"])
        self.assertEqual(section["list"]["kept"], {"missing": 1, "wanted": 4})
        self.assertEqual(section["list"]["skipped"], {"ecap": 1})
        self.assertEqual(section["photos"]["compared"], 7)
        self.assertEqual(section["posters_without_a_compared_face"], {"count": 1, "reasons": {"off_fbi_gov": 1}})
        self.assertEqual(section["impostor"]["poster_x_lfw"]["false_match"]["trials"], 7 * 5)
        self.assertEqual(section["genuine"]["posters_with_two_or_more_compared_photos"], 3)
        self.assertEqual(section["quality"]["every_face"], {"compared": 7, "why_small": 1})
        text = json.dumps(section)
        for item in items:
            self.assertNotIn(item["title"], text)
        self.assertNotIn("@@images", text)
        self.assertFalse(rf.poster_gate(section, True)["allowed"])
        self.assertEqual(rf.measure_posters(type("A", (), {"no_posters": True})(), None, None, 0.63, None)["measured"], False)

    def test_the_cutoff_rule_is_its_own_failure(self):
        curve = with_posters(synthetic_curve(), section())
        curve["threshold"] = curve["measured_cutoff"] - 0.01
        self.assertTrue(curve["real_posters_allowed"])
        self.assertTrue(any("under the measured cutoff" in f for f in rf.full_run_rules(curve)))


if __name__ == "__main__":
    unittest.main()
