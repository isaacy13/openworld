# OpenWorld v0 — threats, gaps, and tightened design

Phone is the audience. A long estimate suggests a computer; the user can still run the job on the phone. **Import-only** reviewer: decode → nano detect → track → quality gates → embed/OCR on hits → locked cosine vs a **frozen, hashed FBI poster cache**. Never asserts identity, clearance, authenticity, or “found.” Does not host the gallery or file tips. After a flag, a deliberate tap opens **that record’s official FBI URL**.

Does not reopen: no KYC; no 30-day media ban (warn only); no LE pattern tool; no Jev/Laya/Clef in the pixel loop; no auto-report; no hidden capture. See [project context](project-context.md), [trust](trust.md), [intake](frictionless-ux.md). Clef (Cloudflare) has a vision encoder and open weights; it still is not the scanner — [feasibility](feasibility.md).

## 1. Threats — all bounded

Matcher uses only the job snapshot of FBI posters the device fetched (plus a plate if that record publishes one). No user watchlist. No in-app form.

**Models.** A bundle is a folder: manifest, ONNX files, that bundle’s locked threshold, and a one-line **best for**. The manifest describes the detector, embedder, plate nano-detector and crop OCR, preprocess, and the 64 px detect / 112 px original-crop gates. Hashes required. The Rust core loads any bundle the manifest describes. Add or remove a model by adding or removing a folder. No scan-code change. The pick is the whole folder, not files mixed from two bundles. Before Analyze, the user picks a bundle. The row shows the name, the best-for line, and whether an FMR/FNMR curve exists. The time estimate matches the pick. Scores are not comparable across bundles. Results name the bundle. No weight download at Analyze. The official UI does not load a random ONNX from disk. Unofficial or developer manifests stay labeled unofficial. File hashes live in git and the GitHub release, same as the app binary checksum; check them at launch and Analyze start; mismatch → refuse. Show bundle name, id, and hash on confirmation and results.

v0 **default** (pre-selected): **Fast** — SCRFD-0.5GF + small ArcFace. Best for phones and long video. A computer does not silently switch off Fast.

The v0 catalog also lists **Accurate** as an addable slot, not a silent swap: larger SCRFD + larger ArcFace (ResNet-100 class). Best for a computer when they want fewer misses. Selectable. If its curve does not exist yet, the row says **not measured yet**. That best-for line is the goal of the slot. It must not be described as more accurate until the curve exists. Plates stay inside each bundle (nano detector + crop OCR), not a separate face model. A later OCR bundle can be another folder.

**Gallery.** No OpenWorld server, rehost, or re-sign; poster distribution costs $0. Explicit **Update posters** (HTTPS `api.fbi.gov` / fbi.gov images, conditional GET; hash the response body, ignore checksum headers). That hash detects a later on-disk edit; it does not prove the photo is the right person. At Analyze start, write-once job snapshot `{record_id, image_sha256, embedding}` + snapshot hash. Matcher may read **only that snapshot** (including pause/resume). A later Update posters cannot mutate it. A later on-disk swap fails the stored hash. Results show fetch time, count, snapshot hash.

**Integrity.** GitHub releases are the distribution. No always-on infra, and no hosted release key for an FBI record package. A fork can ship any gallery; open models mean they do not need our binary. Poisoned posters on other official installs are not a v0 infra problem without a compromised FBI origin or a different build.

**Sighting map.** An open map of reports everyone can see is a map of **user sightings** (photos, face crops, pins). Poisoning the FBI download on other phones is still not a v0 infra problem. No OpenWorld server rehosts posters. That map is where poisoned or fake reports reach other people. **Out of v0.** It needs a server, moderation, and it fights no-upload, no match-sharing, and no public accusation. Allowed later, if ever: a map of **locations the agency already published** on the official poster (last-seen city/text). No user-uploaded media, no face crops from scans, no “someone reported a sighting here.”

| Item | Status | Rule |
| --- | --- | --- |
| Silent model / mid-scan gallery swap | **Closed** | Release file hashes + write-once snapshot of the local body hash |
| Sparse scan as `complete` / not-compared as cleared | **Bounded** | Misses expected. Name the policy. Not compared ≠ cleared. Empty ≠ “not on the FBI list.” |
| Hidden faces / plates | **Bounded** | Expected miss. Not an anti-disguise product. |
| Unauthenticated file as proof | **Bounded** | Same as existing fake reports. We don’t file or mint a certificate. On-device ≠ authentic. |
| Stale/wrong official poster | **Bounded** | Wrong FBI photo: disclose, can’t audit. Stale *our* copy: drop resolved/captured/deceased/surrendered; 24h banner; 7-day refuse; job snapshot. Label age-progressed as composite. |
| Threshold shopping | **Bounded** | Same locked gate. No slider, raw cosine, or near-miss list. |
| False FBI tips | **Bounded** | Agency intake already exists; we don’t file. |
| User watchlist | **Impossible** | Matcher reads only the FBI snapshot. |
| Stalk / bulk identify the public | **Bounded** | Tiny official poster set only — same info as fbi.gov. |

Coverage skips that all look like “no flag,” and are allowed: quality reject (seen, **not compared**); `measured` cadence (never sent to detector); below locked gate (high FNMR on purpose).

## 2. Technical gaps vs the cascade

```text
decode covered slices → nano face / vehicle / plate detect → track
  → quality gates → embed / OCR on eligible hits
  → locked cosine vs tiny public gallery → review; optional official link
```

Jev/Laya/Clef stay out of this loop.

**Runtime.** One Rust library does hash verify, coverage ledger, quality gates, tracking, and locked cosine, and links ONNX Runtime. Laptop and phone use that same loader. A bundle folder’s manifest names the ONNX files (name, version, SHA-256, license), that bundle’s locked threshold, and a one-line best for. The core loads any folder the manifest describes. v0 pre-selects Fast on both; the user can pick another listed folder, and the estimate follows the pick. Weights stay out of git. iOS uses ORT’s Core ML provider; Android uses NNAPI or QNN. Desktop decode is FFmpeg; phone decode is AVFoundation or MediaCodec. The laptop CLI is that core; Swift and Kotlin only call it.

**Long video.** Streaming decoder, coverage ledger, disk-backed checkpoints, bounded crop cache, and a preflight estimate (codec, duration, resolution, disk, wall time, heat, battery). Show the estimate for the detection size they pick and the bundle they pick. A long one suggests a computer; the user can still start `complete` on the phone. Heat **pauses**. A paused or partial job is not `complete`. Never drop frames to keep that label, and never silently switch the job to `measured`. Not enough disk, a bad hash, or an unsupported codec refuses the start — not a fake no-candidate. Audio discarded.

**Detection size.** Before Analyze, ask. Smaller frame (about 320–640 px on the long side: faster, misses more small or distant faces) or full resolution (slower). No silent default. The resize is in memory only. It does not re-encode the file, and it is cheap next to the detector. Show the time estimate for the option they pick, and on a phone the heat and battery. A **64 px** short side on the **detection** image means the detector saw a face; those tracks appear in the inventory. Comparison (embedding + locked cosine) requires the crop from the **original** frame to be at least about **112 px** short side (ArcFace input size; 80 px is the low end; 112 is the v0 compare gate). Below that: **not compared**, never cleared, never a candidate. On a ~640 px detection frame, a 64 px box usually maps to a much larger original crop, so it often passes; on full-res 4K, a 64 px face usually fails the 112 px original-crop gate. Evidence crops still come from the original frame when available. The UI shows the resolution they chose. A long estimate suggests a computer; it does not block the job.

**Phones.** Same locked gate as desktop for a given bundle (looser phone threshold = shopping). No duration, resolution, or file-size cap on `complete`. Foreground-only; OS kill → incomplete. No camera/mic in v0. Fast is pre-selected on phone and computer. Picking another listed bundle, including Accurate, does not add a phone time ban. Warn from the provider ORT loaded, not a hardware inventory. iOS: A11 and later have the Neural Engine; warn when Core ML / ORT falls off the ANE. Android has no reliable has-NPU bit: neural shows no warning; GPU gets a short note; CPU gets a pre-Analyze heads-up (slower, warmer, more battery) and the scan stays allowed. Computer: Apple Neural Engine or GPU on Mac; CUDA, DirectML, or a vendor NPU (Intel/AMD/Qualcomm) on Windows when it loads; else CPU. Warn only when the loaded path is CPU; a shorter note if GPU is clearly slower. Suggest a computer when the estimate is long because of cooling and power, not because phones lack a neural chip. Thermal pause still applies.

**Calibration / eval.** Operating point for *this* tiny 1:N, jointly with quality + multi-frame. Conservative FMR, accept high FNMR. The measured curve is part of v0, one curve per bundle: FMR/FNMR with counts, detection recall at the 64 px rule, a count of faces seen but not compared, and the ~1 s miss rate under both coverage policies ([validation](validation.md)). Curve presence is data on that bundle. If it does not exist yet, the picker row says **not measured yet**, and the bundle must not be described as more accurate. Real poster matching for a bundle stays off until that bundle’s curve replaces the placeholder. v0’s pre-selected Fast mix is not done until its curve exists. Scores are not comparable across bundles. Phone thermal/RAM, expiry/hash-fail, and airplane-mode still block a public app. Plain list: [release gates](release-gates.md). Real public photos only after a versioned fixture bundle ([trust](trust.md)). No per-user threshold. No fairness claim where fixtures can’t support it.

**Fail closed**

| Condition | Behavior |
| --- | --- |
| No FBI cache, >7 days old, or gallery hash mismatch | Matching **off**; offer **Update posters** |
| Model / preprocess / threshold hash ≠ git / GitHub release | **Refuse** Analyze |
| Unsupported / unreadable file | **Refuse**; not a no-candidate |
| Not enough disk to checkpoint | **Refuse** to start |
| Heat, battery, OS pause, crash | Pause/abort; ledger shows the hole; **incomplete**; do not relabel as `measured` |
| Original crop under ~112 px, or confidence / blur / pose / occlusion / exposure fail | **Not compared**; never cleared, never a candidate |
| Below locked gate | Not listed; **not** cleared |
| Plate in media but not on an enabled record | Do not OCR / store plate text |
| Vehicle cue only | No person-candidate |
| User cancels | Delete in-progress crops/templates unless they keep the job |

## 3. Tightened v0

| In | Out |
| --- | --- |
| User-selected stills and imported video | Live camera, background capture, in-app recorder |
| Desktop drag-drop / picker; phone picker / share-sheet | Watch folders, lock-screen work |
| Local cascade + inspectable FBI snapshot | Jev/Laya/Clef in the pixel loop; clipboard note / form prefill |
| User-picked bundle folders; Fast pre-selected; other slots are folders, not silent swaps | Official UI loading a random ONNX from disk |
| Deliberate tap → **that record’s** FBI `url` | Clipboard, auto-submit, Amber/WEA replacement, match sharing |
| Typed `missing` / `wanted` faces; plate OCR only if that record publishes the plate | User gallery, internet face search, police-only data, ECAP, unidentified remains, audio ID |
| Warn: old media, timestamp conflict, unauthenticated pixels | KYC; 30-day scan ban; LE cross-video patterns |

**Coverage — two named policies**

1. **`complete`** — every decoded frame to the detector, on phone or computer. The user can choose it after seeing the estimate, including a long phone job. If the run does not finish every frame, it is not `complete`. No quiet sample. No silent switch to `measured`.
2. **`measured`** — only if the user explicitly chooses it. Documented cadence (initial **5 Hz** + tracker). Persistent banner: brief appearances may be missed.

v0 size rule: **64 px** short side on the **detection** image puts the track in the inventory; compare only when the **original** crop is about **112 px** short side (else **not compared**, never a candidate). Still unmeasured (replace before real-record release): person-candidate = locked gate **and** ≥ **3** quality frames **or** **1 s** eligible track (until eval: do not enable real records). A heat pause does not count as `complete` until every decoded frame has been sent.

**Quality.** Hard blocks, not settings: original-crop short side (~112 px to compare), detector confidence, blur, pose, occlusion, exposure. No “force match.” Inventory shows rejects.

**Flag CTA.** Deliberate tap opens that record’s official FBI `url` only. No clipboard, inject, query prefill, or form.

| Flagged | Open | On-screen (not a second form) |
| --- | --- | --- |
| `wanted` (incl. parental-kidnapping **abductor**) | That poster, e.g. [Singh](https://www.fbi.gov/wanted/topten/satinderjeet-singh) | Do not approach. Typically **1-800-CALL-FBI** (1-800-225-5324) / [tips.fbi.gov](https://tips.fbi.gov/home). |
| `missing` | That poster, e.g. [Kandil children](https://www.fbi.gov/wanted/kidnap/amina-and-belel-kandil) | Do not approach; not a criminal accusation. Missing-child posters typically **NCMEC 1-800-THE-LOST** (1-800-843-5678). |

Checked 28 Sep 2026 on live FBI HTML. Open **the poster URL**; a plain public tips link only where this table already names it, so missing cases are not framed as fugitive tips. Hand-off: **You are leaving OpenWorld. OpenWorld is not sending a report.** Recency / unauthenticated-file warnings on the way out.

**Results** die with the package or after **7 days**, whichever is sooner. Tombstone + re-scan offer. Embeddings follow the result. Imports stay until the user deletes them. No cloud biometric backup.

| Event | v0 |
| --- | --- |
| File ≳ 30 days | **Warn**; still scannable |
| EXIF / mtime / clock disagree | **Warn**; still scannable |
| Suspected synthetic | **Warn only** |
| Expired / bad package or models | **Block** matching |
| Long or huge file | Estimate shown; user may choose `complete`; `measured` only if they pick it |
| Incomplete job | **Block** reading as “no candidates” / clearance |
| Official link | Never automatic |

Before Analyze: [trust](trust.md) disclosure plus this job’s coverage name, an explicit detection-size choice (smaller frame or full resolution), FBI snapshot time/count/hash, and the bundle pick. The row shows that bundle’s name, best-for line, and whether an FMR/FNMR curve exists. The time estimate matches the bundle pick and the detection-size choice (heat and battery on a phone). On-screen: file not authenticated; on-device ≠ true; candidate ≠ identity / guilt / “found”; missing ≠ accusation; no candidate ≠ clearance. No match-card export.

v0 succeeds if a user can finish a disclosed local scan, inspect crops and the actual posters used, and choose whether to open an official page, and the measured curve for the pre-selected Fast bundle in [validation](validation.md) exists. An unmeasured catalog row says **not measured yet** and is not that curve. A fixture CLI alone is not v0. Real poster matching for a bundle stays off until that bundle’s curve replaces the placeholder. It fails if the UI says “identified,” “cleared,” or labels a sparse pass `complete`.

## 4. v1+ (not v0)

Parked: in-app recorder / live camera (non-hideable indicator); opt-in watch folder as a *queue*; C2PA / deepfake detector; textual Jev/Laya/Clef *after* local CV; audio presence-of-speech only; extra sources after terms (NCMEC after FR notice; other federal HTML most-wanted first-party only, never OpenSanctions; INTERPOL only with written photo-reuse permission; Amber/Silver `alert` overlay per [record classes](record-classes.md)); NTOS TIPS partner only if we accept becoming a report channel; independent security/CV *claims* after the review. Map, if ever: agency-published last-seen city/text only — no user sightings, uploads, or scan crops.

**Never:** KYC as cheat control; 30-day media hard-ban; LE cross-video pattern finding; user watchlists / “search this face”; auto-report, match certificates, hidden capture, scraping/POSTing tips.fbi.gov without NTOS; training on user or poster photos; ECAP, unidentified remains, sex-offender registries, “find my family.”

## Needs user

None that reverse locks. The measured curve is part of v0. Legal/source clearance still gates a public app that matches real posters. Later forks only: in-app recorder; NTOS TIPS partner (reverses no-submit / no-upload).
