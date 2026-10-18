# Release gates

Two lists. The fixture CLI can still be written first: synthetic or consented media, a placeholder threshold, and no live FBI faces. v0 is not done without the measured curve. A public app that matches real posters waits on the second list.

What the curve has to contain: [validation](validation.md). The reviewer checklist — what a release ships, how to check a binary, and what an eval may claim — is in [trust](trust.md). Product locks: [project context](project-context.md).

## Must exist for v0

### Measured FMR/FNMR

False-match and false-non-match rates, with counts and a threshold curve, plus detection recall at the 64 px rule, and the miss rate for a ~1 s appearance under both `complete` and `measured` (5 Hz).

v0 includes this eval. The cosine value in the bundle starts as a placeholder so the pipeline can be tested. That run is not the accuracy claim. Real poster matching stays off until the measured curve replaces the placeholder. A candidate is still not an identification. One public clip is not this measurement.

## Still blocks a public app

### Legal review

A review of the product for each jurisdiction and each source, including a data-protection impact assessment where the law requires one.

It blocks a public app because comparing a face to a real poster is a biometric check on an identifiable person. Shipping that before the review can break privacy law or a source’s terms.

The fixture harness does not call `api.fbi.gov` and does not load real missing or wanted faces. Source clearance still has to allow **Update posters** before real records are on.

### Phone thermals

Heat, speed, and memory measured on named phones. A laptop run is not that measurement.

It blocks a public app because the phone is the audience. A build that throttles, stalls, or calls a hot pass `complete` would misstate what was scanned.

The first code is the laptop CLI on fixtures. Long video belongs on the laptop. Phone thermals are proof for the phone build.

### Signed binaries

Signed release binaries, checksums, a software bill of materials, and provenance. How to check them is in [trust](trust.md).

It blocks a public app because the install people download has to be the build that matches the tagged source and the hashed models.

The harness is a local developer CLI on fixtures. It is not a published install.

### Independent audit

An independent security review, and an independent computer-vision review — or a plain statement that the vision review was not done.

It blocks a public app because publishing the source is not a review of the threat model or the error rates.

The harness is not a public release and does not match real posters.

Usability is the same kind of gate: on-device and an empty result are not proof, and the screen does not nudge anyone to approach. The wording lives in [trust](trust.md).
