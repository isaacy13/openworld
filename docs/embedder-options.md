# Embedder options: can we beat SFace on cross-age/cross-pose and still ship?

Status: research note, 2026-10-22. Decision-oriented survey for the owner. Not a
decision and not legal advice. It does not change any locked spec; [decisions.md](decisions.md)
and [legal-review.md](legal-review.md) still govern.

## Why this exists

Fast's embedder is **SFace 2021dec** ([decisions §2](decisions.md)). At the locked
0.63 cutoff (false-match rate ~8e-7 on LFW), it is accurate on frontal, same-era
faces but weak across age and pose — exactly the conditions FBI poster photos and
user snapshots present. From [`bundles/fast/curve.json`](../bundles/fast/curve.json):

| Held-out set (at 0.63) | SFace miss rate | Note |
| --- | --- | --- |
| CALFW same-person pairs (cross-age) | **73.0%** missed (2190/3000) | FMR 0 on this set |
| LFW one-pair-per-person (cross-age-ish, same-era) | **26.1%** missed (424/1623) | the single-photo miss headline |
| CPLFW same-person pairs (cross-pose) | **97.9%** missed (2936/3000) | most pairs never even compared (pose gate) |
| LFW ten-fold accuracy | 0.964 | CALFW 0.9122, CPLFW 0.5417 |

The cause is weak genuine/impostor separation: to hold FMR at 1e-6 the cutoff must
sit at 0.63, and at that cutoff most cross-age genuine pairs fall below it. A better
embedder would push genuine cross-age scores higher **relative to** impostor scores,
so the 1e-6-FMR operating point keeps more true matches. That — not raw LFW accuracy —
is what we are shopping for.

## The gate

A candidate is only usable if **the weights' license permits redistribution inside a
shipped commercial app** (the app is free, but OneX is a company, so "non-commercial
research only" terms fail). [legal-review §9](legal-review.md) adds a second, deeper
test that the weights' *license* alone does not settle: the **training-data
provenance**. It flags **MS-Celeb-1M / MS1MV2 / MS1MV3** (withdrawn by Microsoft in
2019, disputed terms), **Glint360K** (cleaned from MS-Celeb-1M + Celeb-500k),
**WebFace260M/42M/4M** (research-only, signed agreement), and **CASIA-WebFace** and
**VGGFace2** (distributed for non-commercial research) as problems. An Apache/MIT
license on the weights file does not grant rights in the data they were trained on.
Both tests must pass.

## Candidates

Accuracy numbers are the authors' published figures on standard benchmarks (celebrity
press photos), **not** our pipeline's numbers and not comparable to the SFace table
above until re-measured by [`eval/real_faces.py`](../eval/real_faces.py). Input is the
aligned crop size.

| Model / variant | Arch | Input | ONNX | Size (fp32) | Weights license — **commercial redistribution?** | Training data (provenance flag) | LFW / CALFW / CPLFW / IJB-C(TAR@1e-4) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **SFace 2021dec** (current) | MobileFaceNet + SFace loss | 112×112 | yes (ships) | ~37 MB | **Apache-2.0 — yes** ([OpenCV Zoo](https://github.com/opencv/opencv_zoo/blob/main/models/face_recognition_sface/README.md)) | **Unconfirmed**; SFace paper used CASIA-WebFace / VGGFace2 / MS1M ([arXiv 2205.12010](https://arxiv.org/pdf/2205.12010)) — §9 says "ask OpenCV/authors which" | 0.9940 (zoo) / — / — / — |
| **AdaFace R100** (WebFace4M) | ResNet-100 + AdaFace loss | 112×112 | exportable (PyTorch; no official ONNX) | ~250 MB | code **MIT**, but **weights NO**: card says follow the training-dataset license ([mk-minchul/AdaFace](https://github.com/mk-minchul/AdaFace), [CVLFace card](https://huggingface.co/minchul/cvlface_adaface_ir101_webface4m)) | **WebFace4M** (research-only) | 0.9980 / 0.9605 / 0.9463 / 97.39 |
| **AdaFace R50** (WebFace4M) | ResNet-50 + AdaFace | 112×112 | exportable | ~170 MB | same as above — **NO** | **WebFace4M** (research-only) | 0.9978 / 0.9598 / 0.9417 / 96.98 |
| **AdaFace R18** (VGGFace2) | ResNet-18 + AdaFace | 112×112 | exportable | ~90 MB | same — **NO** | **VGGFace2** (non-commercial research) | 0.9947 / 0.9390 / 0.9172 / 92.95 |
| **ArcFace R100** (InsightFace `buffalo_l` / antelopev2) | ResNet-100 + ArcFace | 112×112 | yes | ~166–250 MB | **NO — explicitly non-commercial** ([InsightFace licensing](https://www.insightface.ai/solutions/face-recognition-licensing); [PyPI](https://pypi.org/project/insightface/0.6.2)) | **Glint360K / MS1M** (from MS-Celeb-1M) | ~0.998 / ~0.96 / ~0.93 / ~96–97 |
| **ArcFace MBF** (InsightFace `buffalo_s`) | MobileFaceNet + ArcFace | 112×112 | yes | ~5 MB | **NO — non-commercial** (same license) | **MS1M family** | lower; edge-oriented |
| **MagFace R100** | ResNet-100 + MagFace loss | 112×112 | exportable | ~250 MB | code open; **weights trained on MS1MV2 — provenance fails §9** | **MS1MV2** (MS-Celeb-1M) | ~0.998 / ~0.96 / ~0.93 / ~95–96 |
| **GhostFaceNets W1.3** | GhostNet + ArcFace | 112×112 | exportable | ~16 MB | weights claimed **MIT** (HamadYA) — license may pass, **but** | **refined MS-Celeb-1M** — provenance fails §9 ([paper](https://www.paperswithcode.com/paper/ghostfacenets-lightweight-face-recognition)) | ~0.997 / ~0.96 / ~0.93 / — |
| **EdgeFace-S / XS** (Idiap) | EdgeNeXt hybrid | 112×112 | yes (HF) | ~3.5 / ~1.2 MB | **NO — CC BY-NC-SA 4.0** ([Idiap/EdgeFace-Base](https://huggingface.co/Idiap/EdgeFace-Base)) | MS1MV2 (also NC) | ~0.997 / — / — / high for size |
| **FaceNet** (facenet-pytorch / davidsandberg) | Inception-ResNet-v1 | 160×160 | exportable | ~110 MB | code **MIT**; weights' own terms unstated | **VGGFace2 / CASIA-WebFace** (non-commercial research) ([pkg](https://pypi.org/project/facenet-pytorch/2.1.0)) | ~0.9965 / ~0.90 / ~0.88 / weaker; older, softmax/triplet era |
| **DigiFace-1M-trained** (synthetic) | ResNet + ArcFace on synthetic faces | 112×112 | exportable | varies | dataset CC-BY-NC-ND (**NC**); weights would be own | **synthetic** (clean of scraped faces, but dataset itself NC) ([paper](https://arxiv.org/pdf/2210.02579)) | 0.9617 / 0.8255 / 0.8223 / low — **worse than SFace on CALFW** |

OpenCV Zoo offers no face-recognition model other than SFace; YuNet is the detector.

## What this means

1. **Everything that is clearly better on cross-age fails the gate.** The models that
   would actually lift cross-age recall — AdaFace R100/R50, ArcFace R100, MagFace,
   GhostFaceNets — are all trained on **MS-Celeb-1M / Glint360K / WebFace / CASIA /
   VGGFace2**, the exact datasets [legal-review §9](legal-review.md) flags. Their
   license status then splits two ways, and **both ways block shipping**:
   - *Weights license says non-commercial* — InsightFace ArcFace (`buffalo_*`,
     antelopev2) and EdgeFace. Hard exclusion.
   - *Weights/code license looks permissive (MIT) but the training data is not* —
     AdaFace (authors explicitly bind the weights to the dataset license),
     GhostFaceNets (refined MS-Celeb-1M), FaceNet (VGGFace2/CASIA). §9's provenance
     test fails even where the license text looks clean.

2. **The only weights that pass the strict license gate today are SFace's own**
   (Apache-2.0, already shipping) — and even SFace carries the *same* unresolved
   provenance cloud (its paper lists CASIA/VGGFace2/MS1M; which produced the zoo
   weights is unconfirmed). So SFace is not "clean", it is "already accepted and
   lower-risk because the weights license is permissive and it is already in the
   build". Swapping in AdaFace/ArcFace would *add* risk, not remove it.

3. **Synthetic-trained is the only genuinely scrape-free direction, but it is not
   good enough yet.** A model trained purely on DigiFace-1M scores ~0.83 on CALFW —
   at or below SFace — so it does not buy the cross-age gain, and DigiFace-1M's own
   license is non-commercial. Not a v0 answer.

## Recommendation

**No off-the-shelf embedder is both redistributable and better on cross-age. Do not
swap one in for a shipped build.** Concretely:

- **Keep SFace 2021dec as the shipped Fast embedder.** It is the only candidate whose
  weights carry a redistribution-permitting license, and it is already measured and
  pinned. Accept the cross-age miss rate for now, as [decisions §1](decisions.md)
  already does ("Misses go up; that is accepted").

- **Top 1–2 to pursue, in priority order:**
  1. **Own-trained embedder with clean provenance (the only real path to shipping a
     better model).** Reuse the permissively-licensed *training code and loss*
     (InsightFace training code is MIT; AdaFace loss is MIT) on a backbone we can
     ship (IR-50 or MobileFaceNet for phones), trained on data OneX has rights to —
     a licensed/consented real dataset with genuine cross-age spread, optionally
     augmented with synthetic faces. The weights become OneX's own, with documented
     provenance, and can populate the honest **Accurate** slot
     ([decisions §2](decisions.md)). This is real work and needs a dataset with clear
     commercial terms, which is the hard part; flag it to counsel before acquiring data.
  2. **AdaFace R100 (WebFace4M) as a benchmark ceiling only — not shippable.**
     Run it through the pipeline to quantify *how much* cross-age recall a modern
     margin-loss model would recover at our 1e-6-FMR cutoff. If the gain is large, it
     justifies the investment in (1); if small, it tells us SFace is near the
     practical ceiling for this gallery. Keep the measured curve clearly labeled
     research-only so it can never be mistaken for a shippable bundle.

- **Exclude for licensing (do not bundle, even to test-ship):** InsightFace ArcFace
  `buffalo_*` / antelopev2 (non-commercial), EdgeFace (CC BY-NC-SA), MagFace and
  GhostFaceNets weights (MS-Celeb-1M provenance), FaceNet pretrained weights
  (VGGFace2/CASIA provenance). AdaFace **weights** are excluded from shipping for the
  same reason, even though its *code* is MIT.

## Measurement before adopting anything

Any new embedder is validated the same way SFace was, by
[`eval/real_faces.py`](../eval/real_faces.py), on the same CPU preprocess as the build.
No code change to the scanner: drop the model into a bundle folder and point the eval
at it.

1. **Bundle it.** New `bundles/<name>/` with `manifest.toml` (name, version, SHA-256,
   license, **training-data provenance**), the ONNX weights, and a placeholder
   threshold. The embedder must emit a unit vector of its own length; the core reads
   whatever the manifest describes.
2. **Set the cutoff on LFW.** `eval/real_faces.py --bundle <name>` runs LFW all-pairs
   (~81M impostor pairs) and derives the lowest all-pairs score at FMR ≤ 1e-6
   (`CUTOFF_FMR`), faces enlarged 2×, on the CPU provider — same rule that gave SFace
   0.63/0.5923.
3. **Read cross-age/cross-pose at that cutoff.** The held-out **CALFW** and **CPLFW**
   sets (pinned digests) are scored at the LFW-chosen cutoff, unchanged. The win
   condition is a materially lower CALFW false-non-match than SFace's **73%** and a
   lower LFW one-pair-per-person miss than **26.1%**, *at an equal or lower FMR*.
   A model that only raises LFW ten-fold accuracy but not cross-age recall at 1e-6 FMR
   is not worth the risk.
4. **Detection and scan behavior.** WIDER FACE size-bucketed detection recall is
   unchanged (same YuNet detector), but re-run the still/video scan and stranger-clip
   sections so the scan's scores still equal the library cosines and the fail-closed
   rules hold.
5. **Hardware agreement.** `openworld bench` must show the accelerated providers'
   embeddings stay within 0.99 cosine of CPU ([decisions §6](decisions.md)); otherwise
   the embedder runs CPU-only.
6. **Poster-photo gate stays separate.** Real posters remain off until the
   poster-photo section ([decisions §10](decisions.md)) passes on the chosen embedder;
   a better held-out curve does not turn real posters on.

## Honesty / uncertainty

- Benchmark numbers above are authors' figures on celebrity photos; the cross-age win
  is a hypothesis until re-measured here. NIST FRTE numbers are not our curve.
- SFace's training dataset is genuinely unconfirmed — §9's "ask OpenCV/the authors"
  action is still open, and it applies to the current build, not just to candidates.
- AdaFace weight terms rest on the authors' model-card statement tying weights to the
  dataset license; have counsel read WebFace4M's actual agreement before treating even
  a *benchmark* run as safe to publish.
- "Commercially usable real face dataset with cross-age spread" is the crux of
  option (1) and may not exist on acceptable terms; that is a data-acquisition and
  legal question, not a modeling one.

## Sources

- [OpenCV Zoo — SFace README (Apache-2.0)](https://github.com/opencv/opencv_zoo/blob/main/models/face_recognition_sface/README.md)
- [SFace paper, arXiv 2205.12010 (training data)](https://arxiv.org/pdf/2205.12010)
- [AdaFace repo (mk-minchul/AdaFace, MIT code)](https://github.com/mk-minchul/AdaFace)
- [AdaFace WebFace4M checkpoint card (weights follow dataset license)](https://huggingface.co/minchul/cvlface_adaface_ir101_webface4m)
- [AdaFace paper, arXiv 2204.00964](https://arxiv.org/pdf/2204.00964)
- [InsightFace face-recognition licensing (models non-commercial)](https://www.insightface.ai/solutions/face-recognition-licensing)
- [InsightFace PyPI (research-only models note)](https://pypi.org/project/insightface/0.6.2)
- [EdgeFace model card, Idiap (CC BY-NC-SA 4.0)](https://huggingface.co/Idiap/EdgeFace-Base)
- [GhostFaceNets paper (refined MS-Celeb-1M)](https://www.paperswithcode.com/paper/ghostfacenets-lightweight-face-recognition)
- [facenet-pytorch (MIT code; VGGFace2/CASIA weights)](https://pypi.org/project/facenet-pytorch/2.1.0)
- [DigiFace-1M, WACV 2023 (synthetic; CALFW/CPLFW figures)](https://arxiv.org/pdf/2210.02579)
- [WebFace260M benchmark, arXiv 2103.04098](https://arxiv.org/pdf/2103.04098)
