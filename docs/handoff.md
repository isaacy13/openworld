# OpenWorld v0 handoff

> Corrected on 2026-10-19 by [decisions.md](decisions.md): the cutoff, the Fast weights, the detection size (now automatic), plates (out of v0), and the fail-closed rules. Where they disagree, decisions.md wins.

Paste this to an agent started **from** https://github.com/isaacy13/openworld. This project has no repo attached and cannot push. The GitHub repo is public and empty. Put the code there. Apache-2.0.

## Goal

Ship an open-source app that runs **on the user’s phone or computer**. They pick a photo or video they already have. The device looks for faces (and plates only in a narrow case) and compares them **locally** to a small set of **public FBI missing-person and wanted-person posters**. If something passes a locked cutoff, show the frame, the crop, the poster, and the uncertainty. A tap opens **that poster’s FBI page**. The app does not send a report.

v0 is done when:

1. A scan runs on a fixture (synthetic or consented), Fast bundle, with the face strip and the disclosure.
2. The user can pick a model bundle and a detection size, and sees a time estimate.
3. Native screens exist (start with iPhone + a desktop window; Android second) and feel like that platform.
4. Fast has a measured false-match / false-miss curve. Real FBI photos stay **off** until that curve exists.

## What this is not

Do not build, and do not relitigate:

- A covert or background camera, or a live street scanner. v0 is import-only (picker, share sheet, drag-and-drop).
- A cloud face search, an upload of video or embeddings, or a server we run. Cost target is **$0**. No VPS, no CDN, no signing service.
- Jev, Laya, or Cloudflare Clef in the pixel loop. They are text or huge decision models. They are not the detector.
- Training a vision model on wanted or missing photos, or on user media.
- User KYC. A 30-day ban on old files (warn only). A law-enforcement pattern database. A public map of user sightings.
- Auto-report, clipboard “copy a tip note,” form prefill, or POST to `tips.fbi.gov`. A tap opens the official FBI URL. On-screen text may say they are leaving OpenWorld.
- “Identified,” “cleared,” “found,” or a shareable match card. Empty and incomplete are not clearance. On-device is privacy, not proof the file is real.
- NCIC or any non-public police database. NCMEC, NamUs, INTERPOL, and Amber/Silver alert feeds are not v0. Amber is a phone emergency broadcast, not something we replace.
- Ultralytics YOLO as a hard dependency (AGPL). Use an Apache/MIT nano detector (YOLOX or RTMDet).

## Pipeline

```text
decode the file in order
  → face / vehicle / plate detect
  → track (ByteTrack)
  → quality gates
  → embed or OCR only on hits
  → locked cosine against a frozen poster snapshot
  → show crops; optional tap to the FBI URL
```

One Rust library does the scan. UI shells do not reimplement it.

- **64 px** short side on the **detection** image means the detector saw a face. It stays in the inventory.
- **Compare** only when the crop from the **original** frame is at least about **112 px** short side (ArcFace input). Below that: **not compared**, never a candidate, never cleared. 80 px is the low end; v0 uses 112. On a ~640 px preview, a 64 px box is often a large original crop. On full-res 4K, a 64 px face usually fails the 112 px gate.
- Plates: OCR only if an enabled poster **publishes that plate** and the crop passes quality. A vehicle is not a person.
- Audio is out.

### Coverage and time

- `complete`: every decoded frame goes to the detector. No silent downgrade.
- `measured`: 5 frames a second plus the tracker. Persistent banner: brief faces can be missed. Only if the user picks it.
- No maximum duration. Before Analyze, show estimated time. On a phone also heat and battery. If the estimate is long, suggest a computer. They can still run `complete` on the phone.
- If heat, battery, or the OS stops the job, it is **incomplete**, not “no match.”
- Unsupported codec, bad hash, missing or expired poster pack: refuse. Do not pretend the scan finished.

### Detection size (a prompt)

Ask before Analyze:

- **Smaller frames** (about 320–640 px on the long side): faster, misses more small or distant faces. This is a resize of each decoded frame **in memory**, not a second video file.
- **Full resolution:** slower. The estimate matches the choice.

Evidence crops come from the original frame. The UI shows which resolution was scanned. Smaller may start selected if the question is on screen.

### Hardware

Same ONNX files. ONNX Runtime uses the provider that actually loads:

- Mac / iPhone: Apple Neural Engine via Core ML, else GPU, else CPU.
- Windows: CUDA, DirectML, or a vendor NPU (Intel, AMD, Qualcomm) when it loads, else CPU.
- Android: NNAPI or QNN when it loads, else GPU, else CPU. There is no reliable “has NPU?” bit.

Warn from that loaded path, not from a hardware guess. CPU: before Analyze, say it will be slower, warmer, and use more battery; the scan is still allowed. GPU: a shorter note. Neural chip: no warning. iPhones since the iPhone 8 / X have a Neural Engine; warn only if the model falls off it.

Decode with the platform (AVFoundation, MediaCodec). FFmpeg only on desktop. Don’t copy each frame more than the one resize.

A 60-second clip on a recent phone neural chip is on the order of the length of the clip for the **small** detector (measure it; don’t hard-code the marketing number). Hours-long `complete` is fine if they accept the estimate. The cheap detector **is** the “is this frame interesting?” test. Do not run Laya or a big model to pick frames.

## Model catalog

A bundle is a **folder**: manifest, ONNX weights, that bundle’s locked threshold, and a one-line **best for**. The core loads whatever the manifest describes. Add or remove a model by adding or removing a folder. No scan-code change.

Before Analyze the user picks a bundle. The row shows the name, the best-for line, and whether an FMR/FNMR curve exists. The time estimate follows that pick.

| Bundle | Weights | Best for | v0 |
| --- | --- | --- | --- |
| **Fast** (pre-selected) | SCRFD-0.5GF + a small ArcFace / MobileFace | Phones and long video | Ships. Real posters stay off until **this** curve exists. |
| **Accurate** | Larger SCRFD + ResNet-100-class ArcFace | A computer, when they want fewer misses | Listed and selectable. Row says **not measured yet** until its own curve exists. Do not call it more accurate before that. |

These are the fast practical models, not the 2026 accuracy leaders. Plate detector + crop OCR live **inside** each bundle. Scores are not comparable across bundles. Results name the bundle.

The official app does not open a random ONNX from disk. A developer manifest must be labeled unofficial. Don’t commit huge weights if the license forbids redistribution; pin name, version, SHA-256, and license in the manifest, and document the download. Prefer weights that allow shipping in an app. InsightFace code is MIT; some zoo weights are research-only. Check before bundling.

## Posters and network

- No OpenWorld server. Phones download poster images from `api.fbi.gov` over HTTPS and hash the **response body**. Ignore any checksum header. That hash catches a later edit on disk. It does not prove the photo is the right person. A compromised FBI origin is outside what a checksum can do.
- v0 **code** uses the same snapshot schema (`record_id`, image hash, embedding, class `missing` or `wanted`) with **fixtures**. No live FBI fetch and no real faces until Fast’s curve exists and source use is acceptable.
- Class on/off for missing vs wanted. Not a per-name picker. Same cutoff for both. Skip ECAP, unidentified remains, and unnamed Seeking Information.
- Freeze the snapshot for a running job so a refresh can’t change it mid-scan. Drop posters that the source marks resolved or captured. Don’t match an expired pack.
- Someone replacing photos on their own phone is their own business. The open models can be run without our app. Don’t build a signing server to stop that.
- App **binary**: README quickstart tells the user to compare the release asset’s SHA-256 to the digest on that GitHub release, then verify the signature against the public key pinned in the repo. A checksum file that shipped next to the binary is not the proof. There is no release yet, so do not invent a hash. “Current release: none.”

## Validation (part of v0, not a later nice-to-have)

Publish, for the Fast bundle, on the same preprocess and hardware as the build:

- FMR and FNMR across thresholds, with counts and a confidence interval. Report the comparison count of the tiny poster set, not a million-person vendor claim.
- Detection recall for faces whose short side is at least 64 px on the detection image.
- Miss rate for a face visible about **1 second**, under both `complete` and `measured`.
- A count of faces seen but not compared.

Do not commit faces into git. Do not train on them.

- Labeled Faces in the Wild can show the scorer runs. It is too easy and the wrong kind of photo to set the cutoff. Don’t redistribute it.
- WIDER FACE can score the 64 px detector count (CC BY-NC-ND; no redistribution).
- SCface is the right kind of stills and requires their signed research agreement. Too small for a very low FMR, and it is not the 1-second video test.
- The Luigi Mangione clip is **out**, including as a smoke test. The shooting video is masked. A viral “NYPD mugshot” was fake. One famous case cannot measure false matches. Do not put that video or a “found this person” demo in the repo.
- NIST FRTE numbers are not our curve. Their images are not public.

Until the three counts exist, v0 is not done, and real posters stay disabled. Legal review, phone thermal tables, signed store binaries, and an independent audit block a **public** release. They do not block the fixture CLI.

## Screens

v0 UI is part of the product, not a debug console left for later.

- iPhone and Mac: SwiftUI, Apple’s layout and type.
- Android: Kotlin / Jetpack, Material.
- Windows and Linux: a normal desktop window, that platform’s type and spacing.
- Same steps and the same words. Different controls and motion. It should look like a good app from that platform, and still be OpenWorld.

The path is obvious: choose the file → see what will be scanned and that it stays on device → pick the bundle and the frame size → see the estimate → start → watch progress and the actual face crops → a candidate shows the crop, the poster, and uncertainty → open the FBI page or delete.

Before Analyze, say in plain language: this file stays on device; faces are compared only to the named public pack; nothing is uploaded; nobody is enrolled; we don’t train on it; we don’t contact an agency. On-device does not mean accurate. A candidate is not identity. No candidate is not clearance. The file is not authenticated. Show recording time and poster freshness. If the file is older than about 30 days, or the timestamps disagree, **warn**. Still scannable.

No hidden camera permission in v0. No watch folder. No background scan.

## Repo shape

- `core/` Rust library + a laptop CLI. First thing that runs.
- `bundles/fast/` and `bundles/accurate/` manifests (Accurate may be an empty slot).
- `eval/` Python only for the accuracy scripts.
- `apple/`, `android/`, `desktop/` thin shells that call the core.
- README with the verify-then-run quickstart (no fake checksum).
- LICENSE Apache-2.0. A short ethics note: possible candidate, not identity; no pursuit; no auto-report.

First commit can be the CLI and the Fast manifest without weights, plus how to download them. Don’t block on the iPhone UI, but don’t ship v0 as a terminal-only tool. The screens are in scope.

## Build order

1. Rust CLI: decode, Fast bundle, fixture image and a short video, coverage ledger, 64/112 gates, fail-closed hashes, print the face inventory. No network.
2. Bundle picker data model and the downscale prompt, even if the first UI is plain.
3. SwiftUI shell on iPhone (share sheet + picker) and a desktop window, both calling the core.
4. Measure Fast. Only then wire a real FBI snapshot behind an explicit “update posters” action.
5. Android, then the Accurate bundle’s own curve.

## Locked words for the UI

- “Possible candidate. Not an identification.”
- “Not compared” / “Incomplete” / “No candidate is not a clearance.”
- “You are leaving OpenWorld” on the FBI link.
- Bundle row: name, best for, “measured” or “not measured yet.”
