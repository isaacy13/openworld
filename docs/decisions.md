# Decisions, 2026-10-19

These corrections were approved by the project owner after the production-readiness review. Where they disagree with an older spec in `docs/`, **this file wins**. Everything not listed here stays as written.

## 1. Base rates set the cutoff

A scan compares every face in a file with every enabled poster. At a false-match rate *f*, *N* posters, and *M* compared faces, chance alone gives about *f·N·M* false candidates. At the old cutoff (*f* ≈ 1e-5 on LFW), a 2,000-face video against 500 posters would give about 9 false cards.

- Fast's cutoff is the lowest LFW all-pairs score with a false-match rate at or under **one in a million**, rounded up: **0.63**. Misses go up; that is accepted.
- A video face is a candidate only after it passes the quality gates on **3 frames or across 1 second**, and its template (the mean of its best gated frames) passes the cutoff. A still decides on its one face.
- Every result states the chance line: how many false candidates chance alone gives this scan, from the bundle's measured rate.
- Real FBI posters stay off. Turning them on needs a curve on poster-like photos and a per-scan false-candidate rate the owner accepts (§10).

## 2. Fast ships real, redistributable weights

InsightFace's SCRFD and ArcFace weights are non-commercial research only, so the old Fast (SCRFD-0.5GF + MobileFace) could never ship, and its "curve" measured synthetic markers.

- **Fast is now YuNet 2023mar (MIT) + SFace 2021dec (Apache-2.0)**, pinned by SHA-256 and measured on LFW, CALFW, CPLFW, and WIDER FACE by `eval/real_faces.py`. The separate Open bundle is folded into Fast.
- Fast reads **Not measured yet** until `bundles/fast/curve.json` is produced by that evaluation for this pipeline.
- Accurate stays a selectable, unmeasured slot until redistributable weights exist.
- The demo markers are a build feature (`fixture`), not a bundle curve. `openworld selftest` checks the pipeline's rules on them.

## 3. Detection size is automatic

The detection size is no longer a prompt. The long side is chosen so a face **112 px across in the original** reaches the detector at the smallest size it reliably finds (YuNet: 28 px), never below 640 px and never above the original. Every face the detector finds is listed; it is compared only when its original-frame box is at least 112 px and it passes the quality gates. The 64 px inventory rule is retired. A 12-megapixel group photo no longer loses its 190 px faces.

## 4. One embedding pass per face

The detector still reads every frame `complete` covers. Each track keeps its five best gated frames (sharp, frontal, confident, at least 0.2 s apart); when the face leaves, those frames are embedded once and averaged into one template. Results hold one row per face, not per frame.

## 5. Plates and vehicles are out of v0

Removed from the code, the manifests, the poster schema, and the screens. Reasons: no redistributable plate OCR model existed (the old path read only demo markers), FBI posters rarely publish a plate, and reading every bystander's plate is a privacy cost for almost no benefit.

**If plates come back:** a bundle-level OCR model with a license that allows shipping and its own measured read accuracy; OCR runs only when an enabled poster publishes a plate; plate text is never stored for a plate that did not match; a vehicle is never a person-candidate.

## 6. Scores can differ by hardware

The cutoff is measured on the CPU provider. `openworld bench` times each provider that registers and reports the lowest cosine between its embeddings and the CPU's on the same crops. A provider whose embeddings drift (lowest cosine under 0.99) is not described by the curve, so the embedder always runs on the CPU; only the detector is accelerated. Benchmarks run on Linux and Windows in CI now; Apple devices are measured when the owner's Mac is available.

## 7. Words and screens

- Every sentence comes from the core with a stable code (`docs/api.md`). Shells translate by code and never compare English text.
- No raw score and no near-miss list reach a shipped screen or result file. A candidate shows a band ("Clearly past the locked cutoff").
- "No candidate" is neutral grey, never green. Incomplete is orange. Candidates are amber, not red (`docs/ux.md`).
- The README is a user and developer document. Behavior is specified by `docs/` and checked by tests, not listed in the README.

## 8. Fail closed, enforced in code

- A scan writes only to a new or empty folder, or one it marked as its own. `delete` and `sweep` touch only marked folders.
- "Complete" needs a clean decode **and** a frame count that agrees: a phone that pushes fewer frames than it promised, a header that under-counts, a decode error, a stop for heat, battery, background, or Cancel are all Incomplete, with the reason.
- Poster vectors must be unit vectors of the embedder's length. Poster photos are pinned by SHA-256 inside the snapshot.
- Packs older than **7 days**, dated in the future, or expired refuse. Posters older than a day carry a warning. Results expire after 7 days or with their pack.
- `official = true` holds only for manifests whose bytes this build pinned. Weights load from the bytes that were hashed.
- Untrusted files are read in bounded memory: one frame at a time, headers by seeking, pictures over 120 megapixels refused.

## 9. Build and platforms

- ONNX Runtime is loaded at run time from a library pinned by SHA-256, not downloaded while building. The pins live in one file, `core/openworld-core/onnxruntime.pins`: the PyPI wheel's published digest, and the digest of the library inside it, computed from that checked wheel. `scripts/fetch_ort.py` downloads and extracts with them; `build.rs` builds the one library digest for the target into the core.
- The core hashes the library file before it loads it and refuses any other bytes, with the `runtime` refusal. This holds for every place it looks: `OPENWORLD_ORT_LIB` (when set, the only place), beside the program, `../Frameworks`, `../lib`, and the development copy. On a computer it never falls back to the system loader, because those bytes cannot be checked; a platform without a pin loads nothing. Release packaging never re-signs or replaces the library, because that would change its bytes.
- Android loads the library by name from the app's signed APK (stored uncompressed, mapped in place, so there is no file to hash). Gradle verifies the ONNX Runtime AAR against `android/gradle/verification-metadata.xml` when it builds, and the APK signature covers it after that. iOS and the Mac app link ONNX Runtime statically from the archive `apple/scripts/build-core.sh` pins; the code signature covers it after that.
- Build inputs are pinned too: GitHub Actions by commit SHA (`scripts/pin_actions.py --check` in CI), the Gradle distribution by Gradle's published SHA-256, and every Gradle artifact by `android/gradle/verification-metadata.xml`, which CI regenerates from what the build resolves and compares with the committed file.
- A `v*` tag runs `.github/workflows/release.yml`: it builds the files, lists them in `SHA256SUMS`, writes CycloneDX SBOMs, attests each file's build provenance, and attaches them to a **draft** release. Signing uses the owner's keys from repository secrets and is skipped, with a log line, when they are absent. No key is committed.
- The desktop app is Rust + GTK 4 + libadwaita on Linux and Windows, linking the core in-process. On Windows it looks like a GNOME app; a WinUI shell is a later option.
- Phones push decoded frames to the core (`ow_job_*`); nothing writes frames to disk.

## 10. The poster-photo gate

`eval/real_faces.py` measures the Fast bundle on the real FBI posters too (`poster_domain` in the curve). It reads the public list, keeps the posters the core keeps, and compares their photos with each other and with every LFW face, at the manifest threshold, by the sex and race each poster states. No photo and nothing about one person is committed; only counts and rates are.

`real_posters_allowed` comes from that section alone, and only on a full run:

- The upper 95% bound of the false match rate is at or under **2e-6** for poster x LFW (a poster against the public, the comparison a scan makes), and for poster x poster when it has enough comparisons.
- For every sex or race group with enough comparisons, the upper bound is at or under **1e-5**.
- "Enough" is the count at which zero false matches could pass (about 3.84 / rate: 1.92 million comparisons, or 384,000 for a group). Smaller sets and groups are listed as not enough data; only poster x LFW must have enough.

The owner accepted these two rates on 2026-10-19 (`GATE_FMR`, `GATE_GROUP_FMR`). Changing either is the owner's call. The cutoff rule (§1) stays a separate failure. The picker line says either that real FBI photos stay off or that real posters are on because the poster-photo check passed. Passing it is not legal review or source clearance.

## Still open

Legal review, measured phone heat and battery on named devices, signed release binaries, an independent audit, a demographic evaluation beyond the sex and race the posters state, and the first full run of the poster-photo curve (§10).
