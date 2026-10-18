# Questions before the MVP

Locks below are closed. Adopting the defaults unblocks a fixture-only laptop harness. Living specs stay short: [project context](project-context.md), [design gaps](design-gaps.md), [feasibility](feasibility.md), [record classes](record-classes.md), [trust](trust.md), [intake](frictionless-ux.md), [release gates](release-gates.md), [validation](validation.md).

## Already closed

Do not re-ask these.

- Local cascade only: sequential decode → nano face/vehicle/plate detect → track → hard quality gates → embed or OCR on hits → locked cosine against a write-once, hashed job snapshot.
- Jev, Laya, and Clef stay out of the pixel loop. No “visual Jev,” and no training on user media or poster photos.
- Import-only. No camera, mic, hidden capture, background scan, or live public-space scanning.
- No KYC. No 30-day scan ban (old media and timestamp conflicts warn only). No law-enforcement cross-video pattern tool.
- No auto-report. A deliberate tap opens **that record’s** official FBI URL. Phone numbers and “you are leaving OpenWorld” are on-screen copy. No clipboard, inject, query prefill, or POST.
- v0 gallery is FBI `missing` and named `wanted` only. Class on/off, not name-pick. Same locked gate for both. Skip ECAP, unidentified remains, and unnamed Seeking Information.
- No user watchlist, threshold slider, raw cosine, near-miss list, or match certificate. Plate OCR only when that enabled record publishes the plate. A vehicle cue is not a person-candidate. Audio is out.
- `complete` means every decoded frame. Before Analyze the user picks smaller frames or full resolution; the time estimate is for that choice. A long estimate suggests a computer; they can still run `complete`, including on a phone. No silent downgrade to `measured`. `measured` is a separate warned policy (initial 5 Hz + tracker). Fail closed on missing or expired cache, hash mismatch, bad codec, and incomplete jobs. Not compared, empty, and incomplete are not clearance. A candidate is not identity, guilt, or “found.”
- **64 px** short side on the detection image: the detector saw a face, and the track is in the inventory. Compare (embedding + locked cosine) only when the original-frame crop is about **112 px** short side. Below that: **not compared**, never cleared, never a candidate.
- Models ship as **bundle folders**. A folder holds a manifest, ONNX files, that bundle’s locked threshold, and a one-line **best for**. The manifest can name a detector, an embedder, plate nano-detector and crop OCR, preprocess, and the 64 px detect / 112 px original-crop gates. Hashes required. The Rust core loads any bundle the manifest describes. Add or remove a model by adding or removing a folder. No scan-code change. The pick is the whole folder. Before Analyze, the user picks a bundle. The row shows the name, the best-for line, and whether an FMR/FNMR curve exists. The time estimate matches the pick. Scores are not comparable across bundles. Results name the bundle. v0 **default** (pre-selected): **Fast** — SCRFD-0.5GF + small ArcFace. Best for phones and long video. The v0 catalog also lists **Accurate** as an addable slot, not a silent swap: larger SCRFD + larger ArcFace (ResNet-100 class). Best for a computer when they want fewer misses. Selectable. If its curve does not exist yet, the row says **not measured yet**. That best-for line is the goal of the slot. Do not describe it as more accurate until the curve exists. Plates stay inside each bundle, not a separate face model. A later OCR bundle can be another folder. The official UI does not load a random ONNX from disk. Unofficial or developer manifests stay labeled unofficial.

## Must answer before code

Each default is enough to start. None of them reopens a lock.

| Question | Adopt now | Unblocks building? |
| --- | --- | --- |
| First runnable: laptop harness, or wait for a desktop shell or a phone app? | The Rust laptop CLI on fixtures: cascade, coverage ledger, and fail-closed rules. That library is the core the phone app calls. Swift UI is a thin shell next, not a second pipeline. | **Yes.** |
| Does that harness fetch live FBI posters? | Same job-snapshot schema (`record_id`, image hash, embedding, class). Synthetic or consented fixtures only. No `api.fbi.gov` call and no real missing or wanted faces until calibration and source clearance. | **Yes.** Legal review does not gate the harness. |
| Block on a measured operating point, or code the named provisional gates? | Code the spec initials first: the closed face-size rule above (64 px detection inventory; about 112 px on the original crop to compare; else not compared, never a candidate); person-candidate = locked gate **and** (≥ 3 quality frames or 1 s eligible track); `measured` at 5 Hz. The cosine constant is a bundle field with a fixture placeholder, not a slider. **v0 includes the eval.** v0 is not done until the pre-selected Fast bundle’s measured curve exists. Another bundle is another folder and can be listed before its curve; the row says **not measured yet** and must not be described as more accurate. Real poster matching for a bundle stays disabled until that bundle’s curve replaces the placeholder. | **Yes** for the fixture CLI. The curve is still required for v0. |
| What goes in the empty repo? | A small Rust library — hash verify, coverage ledger, quality gates, tracking, locked cosine — linked to ONNX Runtime (desktop CPU/GPU; iOS via the Core ML execution provider on the Apple Neural Engine; Android via NNAPI or QNN). It builds as a laptop CLI and as the library Swift and Kotlin call. First code is that CLI on fixtures; Swift UI is a thin shell on the same core, not a second pipeline. Models are bundle folders. Each folder’s manifest (name, version, SHA-256, license, locked threshold, one-line best for) points at that folder’s ONNX files. The core loads any bundle the manifest describes; add or remove a folder, no scan-code change. v0 pre-selects Fast — SCRFD-0.5GF + small ArcFace, best for phones and long video — the same folder on laptop and phone. Accurate — larger SCRFD + larger ArcFace (ResNet-100 class), best for a computer when they want fewer misses — is a catalog folder the user can pick, not a silent swap. If its curve does not exist yet, the row says not measured yet, and it is not described as more accurate. Plates (nano detector + crop OCR) stay inside each bundle. The official UI does not load a random ONNX from disk. Unofficial or developer manifests stay labeled unofficial. Do not commit weights. Analyze refuses on hash mismatch. Desktop decode is FFmpeg; the phone uses AVFoundation or MediaCodec. Python is only for FMR/FNMR eval scripts. The repo is ONNX Runtime plus that Rust core, not PyTorch, Clef, or a Python-only loop. [isaacy13/openworld](https://github.com/isaacy13/openworld). | **Yes.** |

## Can start without

These still block a public app. They do not block writing the fixture CLI. The measured curve is not in this list: it is part of v0. Plain version: [release gates](release-gates.md). What to measure: [validation](validation.md).

- Legal review per jurisdiction and source, and a DPIA where one is required.
- Named-device phone thermal and RAM (a laptop run is not that proof).
- Signed binaries, checksums, SBOM, provenance, and an independent security or CV review.
- Usability checks that on-device and empty are not read as proof, and that nobody is nudged to approach.

## Out of scope for MVP

- A second scan pipeline beside the Rust core; in-app recorder, live camera, and watch folders.
- The official UI loading a random ONNX from disk. Unofficial or developer manifests stay labeled unofficial.
- Jev, Laya, or Clef on the scan path.
- Any source besides the FBI poster schema: NCMEC, NamUs, INTERPOL, other federal HTML, state lists, Amber/Silver `alert`.
- Tip filing, clipboard notes, query prefill, and an NTOS TIPS partnership.
- User watchlists, internet or CCTV face search, police-only data, audio identity, match certificates.

## Recommendation

Start the fixture CLI. The product locks are closed, and the four defaults above are enough to open the skeleton. The first build is the Rust CLI on fixtures — the same core the phone app will call — using synthetic or consented fixtures and the provisional gates. Swift UI is a thin shell on that library next, not a second pipeline. [github.com/isaacy13/openworld](https://github.com/isaacy13/openworld) is still empty of that skeleton. v0 is not done until the pre-selected Fast bundle’s measured curve in [validation](validation.md) exists. Real poster matching for a bundle stays off until that bundle’s curve replaces the placeholder, and source clearance still has to allow **Update posters**. Legal review, phone thermals, signed release binaries, and an independent audit still block a public app. See [release gates](release-gates.md).
