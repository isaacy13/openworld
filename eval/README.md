# Measurements

`real_faces.py` measures the Fast bundle (YuNet 2023mar + SFace 2021dec) on public face photos and writes its curve, `bundles/fast/curve.json`. The scan is the Rust library. The scripts only count.

Real FBI posters stay off. The curve says `real_posters_allowed: false`, and nothing here turns them on.

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

The photos are downloaded into `--data` and refused unless their SHA-256 matches the pins in `real_faces.py`. No photo is committed. Only counts, rates, and thresholds are written.

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
  --people 300 --wider-images 500 --scan-people 30 --video-people 5 \
  --check bundles/fast/curve.json > /dev/null
```

It fails unless the score at FMR 1e-4 is within 0.01 of the published one, every sampled rate's 95% interval holds the published rate (LFW false matches included), no stranger still or clip gets a card, and the scan's scores equal the library's. A full run fails on the last two and when the manifest threshold is under the measured cutoff. It still writes the curve first.

`python3 -m unittest discover -s eval -p 'test_*.py'` tests the counting on synthetic numbers.

## What the numbers mean

The base rate matters more than the rate. A scan compares every face with every poster. At an FMR of one in a million, 1,000 posters and 1,000 faces are a million comparisons: about one false candidate from chance alone. The curve's `chance.per_scan` table gives the numbers and their upper bounds. A candidate is not an identification.

They do not mean:

- LFW, CALFW, and CPLFW are celebrity press photos. They are not mugshots, posters, CCTV, or NIST. NIST FRTE numbers are not this curve.
- LFW has no demographic labels, so there is no subgroup analysis. Rates by age, sex, or skin tone are not measured.
- LFW faces are enlarged from about 94 px. This is faces near the 112 px compare size.
- The clips are stills moved on a gray screen, not real footage.
- The rates are on the CPU. Another provider is described only as far as `openworld bench` shows its scores match.

`open-curve-2026-10-18.json` is the earlier Open bundle's curve, measured on the old pipeline at cutoff 0.51. It is kept for reference. It is not comparable with this one.
