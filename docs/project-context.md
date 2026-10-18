# OpenWorld — project context

## Purpose

Open-source, on-device assistant for a narrow public-safety workflow:

1. User selects a photo or imported video they are entitled to analyze (**v0 import-only**).
2. Device compares eligible faces locally to a small, current package of **public missing- and wanted-person** posters (typed so a missing child is never a fugitive).
3. If quality + similarity gates pass, show the frame and crop.
4. A deliberate tap opens **that record’s official FBI URL**. No clipboard note, prefill, or tip.

Not a general face search, covert recorder, cloud biometric service, Amber replacement, or police-only interface. Defaults: [design gaps](design-gaps.md). Classes/sources: [record classes](record-classes.md).

## Must / must not

**Must:** on-device inference; public attributable records with refresh/expiry; visible review; short local retention; uncertainty + source freshness on every candidate.

**Must not:** background/hidden capture or silent upload; internet/CCTV scrape, training on non-consensual biometrics, or police-only DBs; claim identity, guilt, “found,” probable cause, media authenticity, or clearance; auto-alert, contact LE, or support confrontation; user watchlist / cross-file scan history; threshold slider, near-miss list, or match certificate; public map of user sightings (photos, face crops, pins).

## First prototype

Offline, user-initiated media review — not public-space scanning:

```text
User-selected image / imported video
  ├─ preflight: codec, duration, detection-size choice, disk, hashes, coverage
  ├─ sequential decode + named coverage (complete | measured)
  ├─ detectors: face (SCRFD/RetinaFace-class); vehicle (YOLO cue only);
  │    plate OCR only if a public reference plate exists
  ├─ quality gates + track/dedupe
  ├─ eligible tracks → align → compact embedding (ArcFace/MobileFace-class)
  ├─ locked cosine vs inspectable FBI snapshot (no slider, no near-miss list)
  └─ review → deliberate tap → that record’s FBI URL (no clipboard, prefill, or POST)
```

Face matching ranks candidates; it does not verify. Vehicle-only ≠ person-candidate. Audio is out.

**Coverage:** `complete` = every decoded frame, on a phone or a computer. Before Analyze the user picks smaller frames or full resolution (smaller may start selected if that question is on screen); the time estimate is for that choice. A long estimate suggests a computer; they can still run `complete`. No silent switch to `measured`. `measured` is a separate choice (initial 5 Hz + tracker); brief appearances may be missed. Empty/incomplete ≠ clearance. File not authenticated.

## Data sources

v0 = FBI Wanted API, classified client-side. Detail: [record classes](record-classes.md).

- Include named `missing` (kidnap / ViCAP missing) and named `wanted` (all warrant-style tags in that list). Skip resolved/captured/deceased/surrendered, ECAP, ViCAP unidentified deceased, unnamed Seeking-Information CCTV.
- Amber/WEA is a broadcast, not a gallery. IPAWS archive is 24h delayed. `alert` overlay is v1.
- NCMEC: right missing-child corpus; inform them of FR first. Do not scrape.
- NamUs: no public photo dump. Do not scrape.
- INTERPOL public JSON exists; terms forbid reusing wanted/missing photos. Don’t pull until they allow it. A Red Notice is not a warrant.
- DEA / USMS / ATF / ICE / USSS / DSS: HTML only → v1+ first-party, never OpenSanctions.
- State/local missing, Canada’s Missing: search UIs; per-source terms later.
- Out: NCIC, stolen-vehicle/plate files, sex-offender registries, sanctions name lists.

Explicit **Update posters** (conditional GET from the FBI). On-device TTL embeddings. Write-once snapshot at Analyze; no gallery fetch then. Expired cache disables matching.
No server, VPS, signing service, or CDN.

**Sighting map (not v0).** An open map of reports everyone can see would publish **user sightings** (photos, face crops, pins). Poisoning the FBI download on other phones is still not a v0 infra problem; no OpenWorld server rehosts posters. That map is where poisoned or fake reports reach other people. It needs a server and moderation, and it fights no-upload, no match-sharing, and no public accusation. Allowed later, if ever: **locations the agency already published** (last-seen city/text). No user-uploaded media, no face crops from scans, no “someone reported a sighting here.”

## Platforms

Shells, shared words, the v0 design bar, and the performance path: [platforms](platforms.md).

| Tier | Scope |
| --- | --- |
| Modern phone (usable NPU/GPU) | Stills and video. Show time, heat, and battery. Suggest a computer when the estimate is long. User may still run `complete`. |
| Low-end phone | Same choice. The estimate is longer. |
| Desktop/laptop | Suggested when that estimate is long. |

Phone is the audience. A long estimate suggests a computer; the user can still run `complete` on the phone. One Rust core loads the bundle the user picks before Analyze: Fast is the default, and Accurate is selectable and labeled **not measured yet** until its curve exists. Do not commit weights. Inference is ONNX Runtime on every platform: desktop CPU/GPU, iOS through the ORT Core ML execution provider (Apple Neural Engine), Android through ORT NNAPI or QNN. InsightFace SCRFD/ArcFace and RapidOCR-class models are already ONNX. Shared scan logic is a small Rust library — hash verify, coverage ledger, quality gates, tracking, locked cosine — linked to ONNX Runtime. It builds as a laptop CLI and as a library Swift and Kotlin call. Python stays only for FMR/FNMR eval scripts. Desktop decode is FFmpeg. The phone uses the OS decoder (AVFoundation / MediaCodec) and does not ship FFmpeg. First code is that Rust CLI on fixtures, the same core the phone app calls. Swift UI is a thin shell on it next, not a second pipeline. A laptop run is not phone thermal or RAM proof.

## Jev / Laya / Clef

Jev is **not JEPA**. It is TypeSafe’s hosted, **text-only** System One API (`choice` / `score` / `noul`). No images, audio, or video ([announcement](https://typesafe.ai/blog/introducing-system-one-models-and-jev), [spec](https://docs.typesafe.ai/models.md)). Not an on-device detector. 70–500 ms is text-request latency. Sending captions to Jev still needs a local vision model and exports media-derived data.

[Clef](https://blog.cloudflare.com/clef-decision-models/) (Cloudflare, 1 Oct 2026) is a **better Jev**, not a better scanner: Jev-API compatible, Apache-2.0 weights ([27B](https://huggingface.co/Cloudflare/clef) / [9B flash](https://huggingface.co/Cloudflare/clef-flash)), and a Qwen **vision encoder**. Hosted Workers AI still **uploads pixels** (max four stills). Local run is documented on an H200, not a phone. It answers typed questions about a state or a few images; it does not return boxes, identity embeddings, or plate OCR. Keep it out of the pixel loop. Detail: [feasibility](feasibility.md).

Optional later: textual policy routing **after** local CV + human review. Not in the scanner. Compact detectors propose; embedding/OCR only on retained hits.

[Laya](https://huggingface.co/convaiinnovations/laya) is a separate Apache-2.0 **text** engine (322M/421M), not a visual detector. Do not train “visual Jev” or Clef-as-matcher for v0. Do not train on missing/wanted photos.

## Recency and KYC

Do **not** refuse files older than 30 days. Show embedded time, mtime, and now separately; warn on conflict. ~30 days = warning before an official link, not a scan ban. Results expire with the package or in **7 days**. No LE cross-video pattern finding.

Do **not** require KYC. The app does not submit reports. False tips are an existing agency problem. v0 warns that it cannot authenticate the file; it does not hard-block suspected synthetic media.

## v0 defaults

Import-only ([intake](frictionless-ux.md)). One locked gate in the signed bundle. Hard quality gates (not compared ≠ cleared). Fail closed on missing/expired cache, model-hash mismatch, bad codec, and incomplete jobs. A long estimate suggests a computer; it does not refuse `complete`, and it does not silently switch to `measured`. FBI-fetched cache + job snapshot; link only (no clipboard or form). Typed `missing`/`wanted`; class on/off, not name-pick. Honesty copy: [trust](trust.md). Jev/Laya/Clef out of the pixel loop.

## Before any real-world test

Legal review per jurisdiction/source; retention/encryption/deletion design; source terms + expiry; independent FMR/FNMR (and disparity where fixtures allow); usability that does not treat on-device or empty as proof; staged rollout on synthetic/consented fixtures — never live public-space scanning. What blocks a public build with real posters: [release gates](release-gates.md).

## Selected sources

- [design gaps](design-gaps.md) · [record classes](record-classes.md)
- FBI [Wanted API](https://www.fbi.gov/wanted/api), [kidnap/missing](https://www.fbi.gov/wanted/kidnap), [TIPS](https://tips.fbi.gov/home), [TIPS PIA](https://www.fbi.gov/file-repository/pias/pia-tips.pdf)
- INTERPOL [Red](https://www.interpol.int/en/How-we-work/Notices/Red-Notices/View-Red-Notices) / [Yellow](https://www.interpol.int/en/How-we-work/Notices/Yellow-Notices/View-Yellow-Notices)
- TypeSafe [Jev](https://docs.typesafe.ai/concepts/system-one.md) · Cloudflare [Clef](https://blog.cloudflare.com/clef-decision-models/) · ConvAI [Laya](https://huggingface.co/convaiinnovations/laya)
- SCRFD [mobile table](https://github.com/deepinsight/insightface/blob/master/detection/scrfd/README.md)
- [AI Act Art. 5](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-5)
