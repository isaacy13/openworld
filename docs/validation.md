# Validation

v0 includes this eval. The fixture CLI can land first, with a placeholder cosine. v0 is not done until the three measurements below exist. Real poster matching stays off until that curve replaces the placeholder. Gates: [release gates](release-gates.md). What an eval may claim: [trust](trust.md).

One famous clip is not the curve.

## What v0 measures

Same model, preprocess, quality gates, and hardware as the build under test. Publish numerators and denominators, the threshold curve, and a confidence interval. The locked cosine is one point on that curve. There is no slider.

**False match and false non-match.** A false match is two images of different people at or above the threshold. A false non-match is two images of the same person below it. The curve is both rates across thresholds. Faces the detector saw and then held back — original crop under about **112 px** short side, or another quality fail — are **not compared**. Count them beside the curve. They are never cleared and never a candidate.

Pairwise FMR is the rate. A scan compares faces with every enabled poster, so the chance of some false candidate grows with that comparison count. Report the count this gate will actually make. The gallery is the tiny FBI set, not a million-person search.

**Detection recall at 64 px.** Denominator: ground-truth faces whose short side is at least 64 px on the detection image. Numerator: those the detector returns. Faces under 64 px are a separate below-policy count. Those hits are compared only when the original-frame crop is about **112 px** short side; shorter crops stay in the seen-but-not-compared count.

**~1 s appearance.** On the same labeled clips, under `complete` (every decoded frame) and under `measured` (5 Hz + tracker). An appearance is an eligible face visible for about one second. A miss is no detector hit for that appearance. `complete` misses are detector or quality misses. `measured` misses add sampling misses. Publish the number of appearances. One clip is an anecdote.

**Poster photos.** The same rates on the real FBI posters: poster against poster, and poster against every LFW face as the public, at the locked cutoff, by the sex and race each poster states. Same-person pairs are one poster's own photos, which include age-progressions and old photos. Posters with no compared face are counted by reason. Real posters turn on only when a full run passes the poster-photo gate ([decisions](decisions.md) §10): the false-match upper bound at or under 2e-6 overall, and at or under 1e-5 for every group with at least 100,000 comparisons. Smaller groups are listed, not hidden.

## Data we can use

Images stay on a local path. The published claim records the protocol name, model and manifest hashes, counts, threshold, and hardware. It does not commit faces, and it does not train on them.

**Method, not their pictures.** NIST’s ongoing [FRTE 1:1](https://pages.nist.gov/frvt/html/frvt11.html) reports FNMR at a fixed FMR on sequestered sets (mugshot, visa, border, kiosk). Reports were still being updated in 2026. Those images are not public. A vendor’s NIST number is not our curve.

**Public pair protocol.** [Labeled Faces in the Wild](http://vis-www.cs.umass.edu/lfw/) has a public pair list (View 2: 3,000 genuine and 3,000 impostor pairs). The site does not grant a commercial license; the photos are news pictures of named people, collected without biometric consent. Do not redistribute them. Modern embedders saturate this set, and 3,000 impostors cannot support a false-match rate near one in a hundred thousand: zero mistakes only bounds the rate near one in a thousand. Use it to show the scorer runs. It is the wrong domain for a surveillance operating point, so it cannot by itself replace the placeholder.

**Surveillance stills, under an agreement.** [SCface](https://www.scface.org/) is 130 people photographed with surveillance cameras and a mugshot camera, released only after a signed [research agreement](https://www.scface.org/SCface_release_agreement.pdf). No redistribution. Right domain, still too few people for a very low FMR, and stills do not measure the 1 s video miss.

**Detection scale.** [WIDER FACE](http://shuoyang1213.me/WIDERFACE/) is CC BY-NC-ND 4.0: non-commercial, no derivatives, no redistribution, no identity labels. Score our 64 px denominator on a local copy. Their easy/medium/hard split is not our recall number.

**Already-held copies only.** NIST [stopped distributing](https://www.nist.gov/programs-projects/face-challenges) IJB-A, IJB-B, and IJB-C on 14 March 2023. Use an authorized copy already in hand. Do not fetch a reupload.

Synthetic faces may debug the script. They are not the published curve. The 1 s miss rate needs video we are allowed to use, with labeled appearance intervals. Until that count exists, that row is open and v0 is not done.

## Mangione-class cases

Public CCTV plus a later mugshot or poster is a **qualitative** check only: the pipeline ran, and a candidate for that record scored above the locked gate. It cannot estimate FMR. There is no counted impostor set. The ending is already famous, so a hit was selected because it happened. A masked frame is an expected miss.

The Mangione clip is **out** as a smoke test. Do not point the harness at it, and do not commit his face, the shooting video, or a named-suspect demo. NYPD released surveillance photos of a then-unnamed suspect after the 4 Dec 2024 UnitedHealthcare CEO shooting, including an unmasked hostel still ([AP](https://apnews.com/article/unitedhealthcare-ceo-shooter-b6a0bcc8f517604bb612511acea164b9)). The shooting video shows a masked face. A Pennsylvania booking photo came after the 9 Dec arrest. A viral “NYPD mugshot” was not an official release ([VERIFY](https://www.ktvb.com/article/news/verify/crime-verify/luigi-mangione-nypd-mugshot-is-fake-fact-check/536-e34a89f7-d486-46e7-9711-2edbfd89060f)).

A smoke test may point at other files the developer already has locally. That run is not the published accuracy claim, and those files stay out of the repo.

Same class, same limit — illustrations, not fixtures:

- **Tyler Robinson.** FBI stills and video of a then-unnamed person after the 10 Sept 2025 shooting of Charlie Kirk; a mugshot followed the arrest ([ABC](https://abcnews.com/US/manhunt-charlie-kirks-killer-enters-3rd-day-questions/story?id=125504696)). The face is often under a hat and sunglasses.
- **Vance Boelter.** FBI wanted poster, 14 June 2025, including a security-camera image of a masked figure ([CBS timeline](https://www.cbsnews.com/minnesota/news/vance-boelter-minnesota-lawmaker-shootings-timeline-melissa-hortman-john-hoffman/)). He was already named. A mask is not a genuine probe.

Skip cases that are still an unidentified person, or where a face was never clearly released.

## What we will not do

- Train on these images, on user media, or on poster photos.
- Ship a match certificate, or treat a candidate as identity, guilt, or “found.”
- Treat one true positive as “it works.”
- Publish a local smoke test as the accuracy claim.
