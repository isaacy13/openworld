# Measurements

`real_faces.py` measures the Fast bundle (YuNet 2023mar + SFace 2021dec) on public face photos and writes its curve, `bundles/fast/curve.json`. The scan is the Rust library. The scripts only count.

Real FBI posters stay off unless the poster-photo check below passes on a full run. Only then does the curve say `real_posters_allowed: true`, and its picker line says why. Every curve so far says `false`.

## What is measured

- **Cutoff.** Every same-person and every different-person pair of LFW photos (about 87 million pairs), faces enlarged 2x. The measured cutoff is the lowest score whose false match rate (FMR) is at or under one in a million. The run fails if the manifest threshold is under it.
- **FMR and FNMR** at the manifest threshold, at FMR 1e-3 to 1e-6, and in a table every 0.02. Each rate has its count, its trials, a Wilson interval, and an identity bootstrap interval (people are resampled, not pairs, because one person's photos are in many pairs). The false matches also report how many people and photos are behind them.
- **Miss rate.** The headline is one pair per person: each person's first two compared photos. That is what one poster against one photo sees. The all-pairs miss rate is weighted toward people with many photos.
- **Quality gates.** The share of LFW photos whose center face is compared, and the count for each gate code (`why_small`, `why_pose`, `why_blur`, `why_exposure`, `why_confidence`, `why_unaligned`), at 2x and at native size. The face nearest the middle of the photo is the labeled person; when it fails a gate, another face is never used in its place.
- **Held out.** CALFW (cross-age) and CPLFW (cross-pose), 6,000 pairs each, at the manifest threshold, unchanged. A same-person pair without a compared face is a miss. A different-person pair without one is reported as not compared, never as a true negative.
- **Scans.** Stills through `openworld scan`: enrolled people found, a wrong poster, strangers carded. The scores the scan reports must equal the library's cosines within 1e-4. Then H.264 clips: the enrolled face on screen for one second while it drifts and grows, under `complete` and `measured`; stranger clips (the false match rate of a multi-frame template); and a two-frame appearance, which must never be a candidate (`too_brief`).
- **Phone files.** A JPEG stored sideways with its orientation tag, a 12-megapixel photo with a 560 px face and one with a 190 px face, and a portrait clip stored on its side with a display rotation. The detection size is automatic, so both 12-megapixel files should be found.
- **Detection.** WIDER FACE validation recall for labeled faces of 112 px and up on the original image (the compare size), 64 to 112 px, and 64 px and up on the detection image, each split by occlusion.
- **Hardware.** The machine, the provider (CPU), and `openworld bench`: timings per provider and how far an accelerated provider's scores drift from the CPU's.
- **Chance per scan.** Expected false candidates = FMR x posters x compared faces, for 50, 200, and 1,000 posters and 10, 100, and 1,000 faces, with the upper bound. `chance.fmr_at_cutoff` drives the line each scan prints.
- **Poster photos** (`poster_domain`, code in `posters.py`). The public FBI list (`api.fbi.gov/wanted/v1/list`), read page by page to its `total` with a pause between requests, classified with the core's own rules (`classify_item` in `core/openworld-core/src/update.rs`; both are tested on `fbi_classify_cases.json`). Up to 5 photos per kept poster and 4,000 in all are downloaded, from fbi.gov only, and compared at their own size through the scan's gates. It reports:
  - false matches poster x poster (different posters, different people) and poster x LFW (every LFW face as the public), with counts, Wilson and identity-bootstrap intervals, and the same by the `sex`, `race`, and `race_raw` the poster states (`unknown` when it is blank);
  - misses between one poster's own photos, which include age-progressions and old photos;
  - posters with no compared face, by reason;
  - the raw `poster_classification` counts, kept and skipped, so a class the rules miss shows up;
  - expected false candidates per scan for the measured poster count and 10, 100, and 1,000 faces.
  Posters with the same name or the same photo file are one person. A poster that names two people (`AND`, `&`) gives no same-person pairs.

### Where the poster run can run

Probed on 2026-10-19: `api.fbi.gov` answers GitHub's runners and a home network, but `www.fbi.gov`, which serves every poster photo, answers 403 to GitHub's runners for every User-Agent, a browser's included. It refuses data-center addresses. The full run therefore runs from a home network (the owner's Mac), not in the measure workflow. The script's User-Agent is `openworld-eval/1`; the earlier `openworld-eval (+https://github.com/isaacy13/openworld)` was refused by `api.fbi.gov` itself.

### The poster-photo gate

`real_posters_allowed` is true only on a full run (no `--people`, no `--posters`, the list read to its total) where the upper 95% bound of the false match rate (the wider of Wilson and the identity bootstrap) is at or under `GATE_FMR` (2e-6) for poster x LFW, and for poster x poster when it has enough comparisons, and at or under `GATE_GROUP_FMR` (1e-5) for every sex and race group with enough comparisons. Enough is the count at which zero false matches could pass, about 3.84 / rate: 1.92 million comparisons overall, 384,000 for a group. Smaller sets and groups are listed under `poster_domain.gate.not_enough_data`; only poster x LFW must have enough. The two rates sit at the top of `real_faces.py`; the owner accepted them on 2026-10-19. The cutoff rule (the manifest threshold may not be under the LFW cutoff) is a separate failure. `poster_domain.gate.failures` says what blocked.

The photos are downloaded into `--data` and refused unless their SHA-256 matches the pins in `real_faces.py`. CALFW and CPLFW are pinned as whole sets, so only a full run checks them; a sampled run says so in its log. No photo is committed. Only counts, rates, and thresholds are written. FBI poster photos are not pinned (the list changes daily); they are saved under `--data/fbi-posters` by their SHA-256 and never cached in CI. No uid, name, URL, or caption reaches the curve. `--no-posters` skips them.

## Run

```sh
python3 scripts/fetch_ort.py                 # pinned ONNX Runtime
python3 scripts/fetch_weights.py fast        # pinned YuNet and SFace
cargo build --release --manifest-path core/Cargo.toml -p openworld-cli --features eval
cargo build --release --manifest-path eval/real/Cargo.toml
python3 eval/real_faces.py --bin core/target/release/openworld \
  --eval-bin eval/real/target/release/openworld-eval --out /tmp/fast-curve.json
```

`--features eval` puts each track's poster scores in the result. Never ship that build. `eval/real` (`openworld-eval`) prints the library's vectors and builds the evaluation poster pack; it is not shipped, and the `openworld` command still has no way to turn a photo into a poster.

Copy the full run's output to `bundles/fast/curve.json` and rebuild: the library pins an official bundle's curve by its SHA-256 at build time.

CI runs a fixed sample against the published curve:

```sh
python3 eval/real_faces.py --bin core/target/release/openworld \
  --eval-bin eval/real/target/release/openworld-eval \
  --people 300 --wider-images 500 --scan-people 30 --video-people 5 --posters 100 \
  --check bundles/fast/curve.json > /dev/null
```

It fails unless the score at FMR 1e-4 is within 0.01 of the published one, every sampled rate's 95% interval holds the published rate (LFW false matches included), no stranger still or clip gets a card, and the scan's scores equal the library's. The poster rates (false matches overall and by group, and misses one pair per poster) are checked the same way when both curves have them; when either lacks `poster_domain` (an older published curve, or the list could not be read) they are skipped with a note. The published `real_posters_allowed` and picker line must be what its own `poster_domain` gives. A sample never turns real posters on. A full run fails on the last two and when the manifest threshold is under the measured cutoff. It still writes the curve first.

`python3 -m unittest discover -s eval -p 'test_*.py'` tests the counting on synthetic numbers.

## What the numbers mean

The base rate matters more than the rate. A scan compares every face with every poster. At an FMR of one in a million, 1,000 posters and 1,000 faces are a million comparisons: about one false candidate from chance alone. The curve's `chance.per_scan` table gives the numbers and their upper bounds. A candidate is not an identification.

They do not mean:

- LFW, CALFW, and CPLFW are celebrity press photos. They are not mugshots, posters, CCTV, or NIST. NIST FRTE numbers are not this curve.
- LFW has no demographic labels. The poster groups are the sex and race the FBI wrote on the poster, not self-description, and age is not measured.
- The poster x LFW rate uses press photos as the public, not the phone photos and video a scan reads.
- LFW faces are enlarged from about 94 px. This is faces near the 112 px compare size.
- The clips are stills moved on a gray screen, not real footage.
- The rates are on the CPU. Another provider is described only as far as `openworld bench` shows its scores match.

`open-curve-2026-10-18.json` is the earlier Open bundle's curve, measured on the old pipeline at cutoff 0.51. It is kept for reference. It is not comparable with this one.
