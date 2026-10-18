# OpenWorld feasibility assessment (2026)

## Verdict

**Partially practical, but not practical as an unrestricted “democratized wanted-person matcher.”**

A privacy-bounded, on-device prototype can scan a user-selected video end-to-end—sequentially decoding its slices and testing frames for faces, vehicles, and plates—then rank *possible* face matches against a **small, public, regularly refreshed, auditable** package of **missing-person and wanted-person** posters. The feasible path is conventional compact computer vision: detection, quality gating, tracking/deduplication, a face-specific embedding model, and local cosine similarity search. It must present uncertainty and supporting crops to a user who chooses whether to open an official reporting channel. Missing-person records are the same matcher with different labels and official-link copy — not a second product ([record classes](record-classes.md)).

This does **not** make Jev the right scanner: it is a hosted, text-only decision API, not an image/video/audio model or detector. Cloudflare **Clef** (Oct 2026) is a better *Jev*—vision encoder, Apache-2.0 weights, Jev-API compatible—but it is still a typed-decision model, not a face/plate detector or verifier, and the hosted Workers AI path **uploads pixels**. It is not technically or legally credible to promise reliable realtime scanning on arbitrary consumer devices, a comprehensive wanted-person corpus, identification certainty, or permission to scan people in public. Public lists are incomplete and volatile; imagery is poor and sparse; face similarity creates harmful false positives; many jurisdictions regulate exactly this sort of biometric processing. No comprehensive open government face database exists for this purpose, and police-only systems (for example NCIC) cannot be substituted.

This is product and technical feasibility analysis, not legal advice or authorization to deploy biometric identification.

## Recommended scope: prototype only

Start with a local-only, deliberate-review workflow:

```text
User-selected photo / full imported video
      ↓
Sequential decode → face / vehicle / plate detection + quality gates
      ↓
Track/deduplicate → face alignment → compact identity embedding
      ↓
Local search of a small, versioned public reference package (typed missing / wanted)
      ↓
Candidate only if conservative multi-signal policy passes
      ↓
Show source frame + crop + source record + uncertainty
      ↓
User may open an official reporting link; no automatic report
```

Important scope cuts:

- No hidden or background capture; visible camera/mic state only.
- No continuous public-space realtime scanning in the first release. Start with full, user-selected imported videos and still images the user is entitled to process.
- No cloud face search, remote embeddings, upload-by-default, automatic notifications, or public match sharing.
- No non-public police data, internet/CCTV face scraping, or user-contributed watchlists.
- No “identified” label: only “possible candidate; manually review.” Never encourage approach, pursuit, confrontation, or public accusation.
- Use plates and vehicles only as supplementary evidence where a reference has been explicitly published and use is permitted. They are not person identity.

## Jev, correctly identified

**Jev is not JEPA.** It is TypeSafe AI's proprietary, hosted **System One** decision model, released in early access in September 2026. TypeSafe describes it as: “[Jev] evaluates typed questions against a state and returns structured results directly. No text generation, no parsing.” Its three outputs are `choice`, ordinal `score`, and yes/no `noul` probabilities; it is meant to route, classify, score, or gate software decisions rather than detect objects or identify people.

> “Jev currently accepts text input only. It evaluates strings, JSON objects, and arrays of text. Images, audio, and video are not supported (yet).” — [TypeSafe System One documentation](https://docs.typesafe.ai/concepts/system-one.md)

The canonical sources are TypeSafe's [launch announcement](https://typesafe.ai/blog/introducing-system-one-models-and-jev), [documentation](https://docs.typesafe.ai/introduction.md), and [model specification](https://docs.typesafe.ai/models.md). The latter specifies Jev 1.13 as text-only with a 64k-token request context and a listed input price of $0.042/M tokens; TypeSafe reports 70–500 ms end-to-end request latency. Those are service claims, not on-device model benchmarks.

| Question | Evidence-backed answer |
| --- | --- |
| Who made it? | [TypeSafe AI](https://typesafe.ai/blog/introducing-system-one-models-and-jev), founded by Diogo Almeida, Erik Gafni, and Sasha Sheng. |
| License / availability? | No public-weight license is offered. Jev runs on TypeSafe's servers through an API; TypeSafe says plainly that the model is **not open source**. Its client tools may be open, but its weights and training recipe are not downloadable. |
| Modalities? | Text only: strings, JSON objects, or arrays of text. **No native image, video, or audio input.** |
| Size? | Undisclosed. TypeSafe has not published a parameter count or model size, so no phone-RAM estimate is justified. |
| On-device story? | None for Jev itself. It is a remote API; the advertised latency includes a network request and cannot establish offline, phone, tablet, or PC performance. |

“System One” here is TypeSafe's product category for fast, bounded, typed decisions—not evidence that every fast visual classifier is Jev-compatible, and not a universal multimedia model class. Calibration is a population-level property; a returned probability cannot make a particular face, plate, or vehicle conclusion correct.

### The likely “laya” / “LIA” reference and other runners-up

The likely intended related project is **[Laya](https://huggingface.co/convaiinnovations/laya)**, not LIA. It is an independent, open-weight System 1-style text decision engine from ConvAI Innovations. The project advertises Apache-2.0 code and weights, typed `choice`/`score`/`noul` outputs, and local execution. Its published checkpoints are still text-only encoders: 421M-parameter English/typed-decision ModernBERT-large variants and a 322M-parameter multilingual variant. The project reports roughly 808 MB for the English download before runtime memory. It can be useful for locally hosted **text** routing, but is not a visual detector and is too large to be a reasonable phone full-video scanner.

The spelling **“LIA”** remains ambiguous: searches found unrelated projects (for example, issue assignment and summarization), not an open visual System One counterpart to Jev. [OpenJev-like projects](https://jevtypesafeai.com/jev/open-source) are also separate community attempts to reproduce a typed-decision interface; they are not Jev, and any claimed image support must be evaluated as that specific project's capability, not as a Jev feature.

## Clef (Cloudflare)

**Clef is a better Jev, not a better OpenWorld scanner.** Announced 1 Oct 2026, it is Cloudflare’s System One–compatible decision family: typed `choice` / `score` / `noul` over a state, drop-in for a Jev client. Unlike Jev, both variants include the Qwen **vision encoder**, and unlike Jev the weights are public.

| | Clef | Clef-flash |
| --- | --- | --- |
| Size / backbone | 27B, [Qwen3.8-27B](https://huggingface.co/Qwen/Qwen3.8-27B) + vision encoder | 9B, [Qwen3.5-9B](https://huggingface.co/Qwen/Qwen3.5-9B) + vision encoder |
| License | Apache-2.0 ([HF](https://huggingface.co/Cloudflare/clef)) | Apache-2.0 ([HF](https://huggingface.co/Cloudflare/clef-flash)) |
| Hosted | `@cf/cloudflare/clef` on [Workers AI](https://developers.cloudflare.com/workers-ai/models/clef/), $0.24/M input tokens | `@cf/cloudflare/clef-flash`, $0.09/M |
| Context (hosted) | 65,536 tokens | 65,536 tokens |
| Documented local run | `torch` on a **single H200** | same recipe |

Canonical: [launch blog](https://blog.cloudflare.com/clef-decision-models/), [Workers AI changelog](https://developers.cloudflare.com/changelog/post/2026-10-01-clef-workers-ai/), [Clef model page](https://developers.cloudflare.com/workers-ai/models/clef/), [Clef-flash](https://developers.cloudflare.com/workers-ai/models/clef-flash/). Cloudflare’s “Jev is 32k, Clef is 64k” line is their characterization of Jev’s **state** budget; TypeSafe’s spec is 64k per request / 32k for `state` plus the longest question.

**What the vision encoder actually does.** It answers schema questions about attached media (“is the receipt total legible?”). The hosted API takes up to **four** embedded PNG/JPEG/WebP stills (4 MiB and 16 megapixels each, 8 MiB decoded total, 13 MiB request; no remote URLs). Local `systemone` also accepts `images` and `videos` as frame arrays. Returns are per-option probabilities. **No bounding boxes, landmarks, identity embeddings, or OCR text.** Cloudflare’s published scores are Decision Index / workflow **text** tasks, not FMR/FNMR or plate-read protocols.

**On-device.** BF16 checkpoints are ~54 GB (27B) / ~18 GB (9B). That is laptop-or-bigger even quantized; it is not a phone NPU detector. A 9B VLM can describe a still. It cannot run every-decoded-frame coverage on hours-long video on the devices OpenWorld targets.

**Hosted Clef fights local-first.** The Workers AI path **uploads pixels**. Cloudflare says it does not train on Customer Content without consent ([data usage](https://developers.cloudflare.com/workers-ai/platform/data-usage/)); that does not make biometric upload compatible with the no-upload cascade. Their RL fine-tune product is also off-device. Do not fine-tune Clef on missing/wanted photos.

### Clef vs Jev vs the v0 cascade

| Need | Clef | Jev | SCRFD / YOLOX + ArcFace + RapidOCR |
| --- | --- | --- | --- |
| Hours-long video, phones/laptops | Hosted: ≤4 stills/request. Local 9B/27B cannot cover every frame | Text API only | Tiny detect + track; embed/OCR only on hits |
| Boxes / crops for review | No localization in the API | No | Detectors return boxes; crop from original |
| Identity vs tiny FBI gallery | Semantic “does this look like…” — not a calibrated 1:N template matcher | Text-only | Locked cosine on face embeddings |
| Plate text | May guess from a still; not OCR | No | OCR on gated plate crops, and only if a record publishes the plate |
| Privacy | Hosted = upload. Local weights exist but are huge | Hosted text | Fully local |

## Jev / Clef verdict for the proposed full-video scan

**Do not use Jev, Laya, or Clef as the video scanner, and do not train “an image/video Jev” (or Clef-as-matcher) for the first prototype.** Jev and Laya take no pixels. Clef takes a few stills (or local frame arrays) and returns schema probabilities, not detections or identity templates. None is evaluated as a face/plate/vehicle detector or face verifier. Sending frames or crops to hosted Jev/Clef is a remote classifier, not a full-video visual scan, and it **exports media**. Hosted Clef is worse than hosted Jev on privacy because the payload can be the pixels themselves.

Jev or Clef could only be an **optional, opt-in textual policy layer after local CV**—for example, classifying a non-biometric textual incident note or routing a manually reviewed result. Neither should decide whether a pixel crop contains a face, whether two faces are the same person, or whether an apparent plate/vehicle warrants action. Local Clef-flash is still the wrong identity model and far too large for the dense pass. Tiny local detectors win on cost, latency, privacy, and localization.

### What should scan the video instead

Use a local cascade, with inexpensive models doing the high-volume proposal work:

```text
Decode the entire selected video in sequential slices
  → inspect every frame or a measured dense cadence
  → tiny local face / vehicle / plate detector
  → confidence + size + blur + pose + visibility gates
  → tracking and temporal deduplication within/across slices
  → only retained face tracks: align + compact face embedding + local gallery search
  → only retained vehicle/plate hits: optional specialist analyzer/OCR
  → conservative multi-frame aggregation and manual evidence review
```

“Full-video” should mean every part of the user-selected file is decoded and covered by an explicit scanning policy. For short-lived faces/plates/cars, only detector inference on every decoded frame can support a no-skip claim; adaptive sampling or tracking reduces cost but can miss appearances between sampled frames. The prototype should expose its actual cadence, skipped-frame policy, and progress, then benchmark recall, sustained throughput, memory, thermals, and battery on named devices.

| Task | Appropriate cheap proposal/filter | Expensive work only after a hit | Why Jev / Laya / Clef do not fit |
| --- | --- | --- | --- |
| “Is there a face here?” | compact SCRFD/RetinaFace-class detector + quality gates | face alignment and embedding | Jev/Laya take no pixels. Clef can score a still (“is there a face?”) but returns no box/landmarks and cannot run per-frame on long video. |
| “Is there a car/plate here?” | small vehicle/plate detector | plate OCR or fine-grained classifier | Same: Clef is semantic classification of a few images, not a detector or OCR engine. |
| “Could this face match a gallery image?” | detector/quality/tracker selects usable crops | mobile ArcFace-style embedding + local cosine search | Typed VLM “does this look like the poster?” is not calibrated face verification. |

For this narrow identity question, use a face-specific verification/retrieval encoder with a documented evaluation protocol, and treat a high cosine similarity only as a candidate-ranking signal. Detection quality, pose, blur, occlusion, age change, image manipulation, duplicate identities, the gallery's source photograph, and threshold calibration dominate actual error.

### Training decision

TypeSafe does not publish Jev's weights, parameter count, architecture details sufficient to reproduce it, training data, or a visual fine-tuning path. Therefore it is not possible to train “your own image/video version of Jev” as an adaptation of Jev. Clef *does* publish Apache-2.0 weights and a Cloudflare RL fine-tune path; that still does not make Clef the scanner, and fine-tuning it for OpenWorld would be a new (and off-device, if using Cloudflare’s trainer) research project.

For an eventual specialist visual model, first establish that the compact detector cascade fails a measured requirement. It would require licensed and representative video, explicit object/face labels for the target task, lawful biometric governance, and held-out evaluation across devices, lighting, pose, occlusion, and demographic conditions. Do **not** start that work with missing-person or wanted-person photos. Start the software evaluation with synthetic or consented fixtures and compare accuracy, energy, latency, memory, and thermal behavior against the detector-plus-embedding baseline.

## On-device runtime reality

### “Any device within reason” needs tiers

There is no meaningful all-device promise. Define performance tiers and publish tested-device results:

1. **Modern NPU/GPU phones and tablets**: may scan imported video locally with compact, quantized models at a measured frame cadence. The product must disclose when that cadence is below every-decoded-frame coverage, and may need to pause on heat or low battery.
2. **Older or lower-end phones**: can support stills and possibly short/slow offline scans. They cannot honestly promise full-frame or realtime coverage of arbitrary videos.
3. **PCs/laptops**: the best first target for complete local batch review of long user-selected files, still entirely local.

An app must budget video decoding, preprocessing, detection, tracking, face alignment, embedding, search, UI rendering, encryption, and heat—not just neural-network latency. It needs an on-screen “analysis active” indicator, an explicit coverage/cadence report, a thermal/battery stop policy, and performance telemetry that never exports media or templates. Jev has no role in this local runtime: it is an API service, not an exportable phone model. Clef’s open weights are real, but 9B/27B VLMs are not the phone/laptop cascade.

### Runtimes

Locked to ONNX Runtime on every platform: desktop CPU/GPU, iOS via the Core ML execution provider (Apple Neural Engine), Android via NNAPI or QNN. One Rust core loads the bundle the user picks before Analyze: Fast is the default, and Accurate is selectable and labeled **not measured yet** until its curve exists. Detail: [project context](project-context.md). NNAPI is device-dependent; do not promise an NPU merely because a model converts. A laptop timing is not phone thermal proof.

Benchmark the *same exported artifact* across target hardware. Use model size, cold/warm latency, memory peak, device temperature, battery impact, sustained throughput, fallback behavior, and candidate/error rates. Fail closed if the accelerator is unavailable or processing becomes too slow/hot.

## CV components

### Faces

Practical pipeline:

1. Detect face and landmarks (SCRFD or RetinaFace-family mobile model).
2. Reject faces that are too small, blurred, occluded, extreme-pose, over/underexposed, or low-confidence.
3. Align the face from landmarks.
4. Generate an L2-normalized compact face embedding (ArcFace/MobileFace-family).
5. Search a local, encrypted reference embedding index using cosine similarity.
6. Require conservative thresholding, multi-frame consistency when video is used, and manual review of the original source image.

Thresholds cannot be copied from an online example. Calibrate them against a consented, representative holdout set and publish false-match/false-non-match tradeoffs. A score must not be converted into an identity assertion.

### License plates and vehicles

A small general detector can locate vehicles and plates; a plate-specific detector plus OCR can read a sufficiently clear plate. This is technically easier than robustly tying a vehicle to a particular person. Plates are privacy-sensitive identifiers; the app should not collect them indiscriminately or match them unless a lawful public reference explicitly provides one. Vehicle make/model/color cues are weak, non-unique supporting context.

OpenALPR-style and similar open models are possible components but have country/region, font, lighting, and license constraints. “Plate Recognizer” is a commercial service/product category rather than a dependency this project should assume is open, offline, or freely redistributable.

### Audio

Audio does not earn a first-release role. Public wanted records rarely contain valid reference audio, and speaker embeddings are highly sensitive to noise, channel, duration, language, accent, age, health, deliberate disguise, and synthetic speech. A voice similarity result is a probabilistic lead—not a unique “voiceprint” or proof of identity—and brings additional biometric/privacy obligations.

If audio is ever explored, limit it first to non-identity tasks such as detecting whether speech is present locally, with no upload, no speaker search, and an explicit user start. Do not build a passive microphone scanner.

## Public matching data

| Source | What it offers | Limitation / design consequence |
| --- | --- | --- |
| [FBI Wanted API](https://www.fbi.gov/wanted/api) | Official public REST records, including some images and updates | Not a national biometric registry; coverage, photos, fields, and status vary |
| [INTERPOL public Red Notices](https://www.interpol.int/en/How-we-work/Notices/Red-Notices/View-Red-Notices) | Selected public notices, public images/record details | Most Red Notices are restricted to law enforcement; a Red Notice is not an arrest warrant |
| US Marshals fugitive pages | Public web profiles | No equivalent public API found; permissions and change detection need review |
| State/local public lists | Some publish web pages or feeds | Fragmented formats, variable update quality/terms; ingestion must be source-specific |
| NCIC and other police networks | Comprehensive operational records | Non-public and prohibited from this project |

There is no comprehensive open, government-controlled face database that a civilian project can lawfully use as a general wanted-person search corpus. Public records should be treated as a limited, revocable set of source documents—not copied indefinitely into a face database.

Reference-package requirements:

- source URL, agency, record ID, publication/update timestamp, content hash, and license/terms field;
- short TTL and deletion/withdrawal propagation;
- signed manifest and rollback/kill-switch;
- provenance shown in the user interface;
- no automatic expansion from webpages, social media, CCTV, or user uploads;
- deliberate source approval before any new agency is added.

## Legal, safety, and ethical constraints

These conditions are architectural constraints, not a disclaimer that makes the product lawful.

- **Biometric law:** Face geometry/embeddings and voice embeddings can be regulated biometric data. Illinois BIPA, for example, requires written notice and consent before collection by private entities, a public retention/destruction policy, security, and restricts disclosure/sale. Texas, Washington, California, and other US regimes can also apply. Camera-recording rules vary separately from biometric rules.
- **EU/UK and similar privacy frameworks:** biometric processing for uniquely identifying a person is sensitive/special-category processing under GDPR-style laws and needs a valid legal basis, necessity/proportionality analysis, data-minimization/retention controls, and typically a DPIA. On-device processing reduces transfer risk; it does not remove the legal issue.
- **EU AI Act:** Article 5 prohibits certain biometric practices, including untargeted scraping to create/expand face databases and, with narrow law-enforcement exceptions, realtime remote biometric identification in publicly accessible spaces. Do not assume a civilian “public safety” framing makes a product lawful; obtain jurisdiction-specific counsel before EU deployment.
- **False-positive harm:** Mistaken candidate matches may lead to fear, discrimination, confrontation, or wrongful reporting. Public photos can be stale, miscaptioned, insufficient, or withdrawn. Conservative thresholds do not eliminate demographic performance differences. The design must discourage action and show evidence/uncertainty.
- **Data stewardship:** A device-local embedding index still contains sensitive biometric information. Encrypt it, avoid cloud backup by default, use source expiry, minimize logs, allow deletion, and never repurpose records for model training or analytics.

Pre-launch requires legal counsel in every target jurisdiction, a data-protection impact assessment where appropriate, source/terms review, consent and deletion design, documented model evaluation, red-team testing, and a clear incident/removal process.

## Development decision

Proceed only with a **planning and evaluation repository** first:

1. Publish the strict scope/limitations and a source-governance policy.
2. Build an offline harness using synthetic or consented test fixtures, not a live scanner.
3. Benchmark a compact detector + face embedding baseline on target phones and desktops.
4. Define quality gates and measure error curves and subgroup performance.
5. Prototype a local, signed public-list manifest with expiry/removal behavior.
6. Conduct legal/source reviews before using any real public record in an app.
7. Only consider a separately trained visual model after the local detector cascade misses a measured requirement, and require a like-for-like baseline comparison.

Do not proceed to live public-space scanning or a general-public release without the legal, source, consent, safety, and measured-performance gates above.

## Technical sources

- TypeSafe AI, [Introducing System One Models & Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev), [System One documentation](https://docs.typesafe.ai/concepts/system-one.md), and [Jev 1.13 model specification](https://docs.typesafe.ai/models.md)
- Cloudflare, [Introducing Clef](https://blog.cloudflare.com/clef-decision-models/), [Workers AI Clef](https://developers.cloudflare.com/workers-ai/models/clef/), [Clef-flash](https://developers.cloudflare.com/workers-ai/models/clef-flash/), [HF Clef](https://huggingface.co/Cloudflare/clef), [HF Clef-flash](https://huggingface.co/Cloudflare/clef-flash)
- ConvAI Innovations, [Laya model card](https://huggingface.co/convaiinnovations/laya) and [project documentation](https://nandhakishorm.github.io/laya/)
- InsightFace, [SCRFD documentation and benchmark table](https://github.com/deepinsight/insightface/blob/master/detection/scrfd/README.md)
- PyTorch, [ExecuTorch Core ML documentation](https://docs.pytorch.org/executorch/stable/ios-coreml.html)
- FBI, [Wanted API](https://www.fbi.gov/wanted/api)
- INTERPOL, [View public Red Notices](https://www.interpol.int/en/How-we-work/Notices/Red-Notices/View-Red-Notices)
- European Commission, [AI Act Article 5](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-5)
