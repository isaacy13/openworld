# Trust must be visible before analysis

The product question: before **Analyze locally**, can the user see what happens to *this* file — which faces, what stays on-device, and whether anything can be sent without their choice? Not whether we have an SBOM. Rules live in the UI, not only a privacy policy. Scope: [project context](project-context.md). Fail-closed defaults: [design gaps](design-gaps.md).

## Disclosure above “Analyze locally”

Specific to the selected item and enabled package. Link out to full detail. Not a compulsory checkbox on every import; must stay reachable during analysis and on results.

> **This video stays on this device while OpenWorld analyzes it.** It will detect face tracks in this selected file and compare eligible local crops only with the small, named public **missing-person and wanted-person** package shown below. It does not upload this video, its face crops, or face templates; add detected people to a database; train on this media; contact an agency; or make a report. Keeping the file on this device does not make results more accurate or prove the recording is real. You can cancel and delete the file and local results.

Inspectable on that screen:

- **Media.** Filename, thumbnail, duration, size, source, coverage (`complete`|`measured`), detect resolution. Embedded time, mtime, now — warn if they disagree. Name local artifacts (crops, temp embeddings, results) and retention (results die with the package or in 7 days).
- **Faces.** A track/crop, not an identity. Quality rejects are **not compared**, not cleared. A candidate does not identify anyone.
- **Comparison set.** FBI snapshot: fetch time, `api.fbi.gov`, counts by `missing`/`wanted`, hash, browsable. Limited public posters — not everyone in the video, not Amber, not police-only, not an OpenWorld gallery. Model-bundle version. Missing, expired, or local body-hash mismatch → matching off. Class on/off is OK; picking names is not.
- **Network.** Analysis is on-device. **Update posters** is a separate, disclosed request (domain, purpose, size, expiry). Never piggyback media/crops/templates/matches.
- **Not against the user.** No enrollment, training, user gallery, shadow DB, sale, background capture, or auto-report.
- **Not proof.** File unauthenticated; candidate ≠ identity / “found” / guilt / probable cause; missing ≠ accusation; no candidate ≠ clearance; on-device = privacy, not accuracy.

## Inspect this scan

1. **Face inventory.** Crops, timestamps, quality/eligibility, compared vs not. Pause/cancel/delete. Excluded ≠ cleared.
2. **Comparison inventory.** Enabled FBI records + freshness beside the faces. User faces never become gallery records.
3. **Candidate, not ID.** Local crop + public record with class badge, source/freshness, coverage, model-bundle, qualitative band (not a tunable score). Label age-progressed. Missing copy: not wanted/guilt; **do not approach** (especially a child). No near-miss list. No match certificate.
4. **End of app action.** Review, delete, or tap **that record’s official FBI URL**. No clipboard, inject, query prefill, POST, or Amber spoof. On the way out: recency, timestamp conflict, unauthenticated file, “you are leaving OpenWorld,” class-correct text, do-not-approach.
5. **Network status.** Local analysis vs any approved Update posters — not a mystery spinner.
6. **Completeness.** `measured`, paused, or unsent ranges → **incomplete**. Incomplete/empty ≠ clearance.

| Promise | Limit |
| --- | --- |
| Selected media, crops, matching, review, deletion are local; no media-derived upload | Not an OS-wide guarantee. Compromised device / other apps / screenshots are out of scope. On-device ≠ accurate. |
| No silent enroll, train, user gallery, shadow DB, or auto-report | Opening the FBI URL is *their* visit; label the hand-off. |
| Matcher uses only the inspectable FBI snapshot | Not everyone in the video; not internet search; not police-only. Public lists can be wrong. File not authenticated. Expiry stops matching. |
| Shows crops, records, uncertainty, delete | Evidence ≠ identification. Candidate ≠ identity. No candidate ≠ clearance. `measured`/incomplete can miss people. |

## Skeptical-reviewer evidence

Open source does not replace the disclosure. Same import-only, on-device scope.

**Ship with every release:** OSI-licensed source + build scripts; threat model (allowed workflow + exclusions + trust boundaries); data-flow (media, crops, embeddings, FBI cache, logs — storage, encryption, retention, deletion, egress); permission inventory (v0: no camera/mic/contacts/broad library/background location/execution); privacy/retention (no train, no sale, no media telemetry, verifiable delete); release notes that never market a candidate as “identified.”

**Supply chain:** signed Git tag + commit; signed binaries + checksums; SBOM; provenance/attestation; reproducible or documented-normalized rebuild in CI; key rotation/revocation.

Stranger check: download binary + checksum + signature + SBOM + provenance + manifest → verify the **app** binary against the GitHub release digest and the release public key pinned in the repo → tag matches commit → rebuild and compare hashes → model-file hashes match git / the GitHub release. Poster images are not in the release. If that fails, don’t claim the binary matches source.

**Models vs gallery.** Models are pinned release artifacts (manifest: URL/license, version, size, SHA-256, conversion, preprocess, runtime). Their file hashes live in git and the GitHub release, same as the app binary checksum. Bundle them or require a named download; check those hashes; never silent-replace weights/thresholds; no user slider. **Gallery is a TTL FBI cache**, not an OpenWorld artifact: explicit Update posters from `api.fbi.gov`, hash the response body, write-once job snapshot at Analyze (pause/resume included), 24h banner, 7-day disable, snapshot hash on results. Poster bytes/embeddings on-device, separate from user media, deleted on expiry. No OpenWorld face CDN, user watchlist, or police-only data.

**Cost.** OpenWorld does not run a server to host or re-sign FBI posters. Phones fetch `api.fbi.gov` over HTTPS and hash the response body locally. Model bundles are signed on a maintainer machine (private key offline) and published as GitHub releases. No VPS, signing service, or CDN. A swap on device fails the stored hash. A swap at the FBI origin is outside what a checksum can detect. Ignore any checksum header. HTTPS authenticates api.fbi.gov’s certificate; it does not prove the poster is the right person. Cert pinning is allowed as hardening. A fork can ship any gallery; open models mean they do not need our binary to do that.

**README quickstart.** Getting started is verify, then run. Download the release asset. Compare its SHA-256 to the digest published on that GitHub release; the README repeats that digest for the current release. Verify the signature against the release public key pinned in the repo. Mismatch or a bad signature: do not run. No release exists yet, so the README does not print a hash — the checksum line is filled from the release. A checksum that ships beside the file from the same server is not the check. This verifies the **app** binary. It is not a poster server.

**Network.** Analyze works offline (models already installed). Default-deny analysis egress. No upload of media, crops, embeddings, scores, or telemetry. Update posters is explicit; show endpoint, purpose, request-only payload, size, expiry. Never mix with Analyze.

Proofs: airplane-mode fixture Analyze; packet capture of import→delete with no connections; egress allowlist only `api.fbi.gov` / documented FBI image hosts during Update posters; logs contain no media-derived identifiers; v0 runs without camera/mic/background capture. These prove the *app*, not a compromised OS.

**Eval.** Claim only: locally ranks **possible candidates** on a named protocol. Synthetic/consented fixtures — not operational user media. Publish: fixture rights; exact models/gates/hardware; FMR/FNMR + CI; detect/quality recall; video miss-rate vs cadence; named-device thermal/RAM; subgroup breakdowns only where fixtures support them (else “unknown”); known fails (small/pose/blur/occlusion/aging/look-alikes/stale source/`measured` misses/unauthenticated media); regression lock. A low aggregate FMR does not identify a person. No below-threshold neighbors in the UI.

**Before public v0:** independent security review (publish scope/date/findings); independent CV review or an explicit “we didn’t do one”; privacy/legal per jurisdiction/source; usability that people don’t read on-device/empty as proof or go confront someone; vuln channel, changelog, source-record removal, rollback.

| v0 may claim, if the evidence exists | Must not claim |
| --- | --- |
| Published build matches tagged source + hashed inputs | Universal trust, certification, “safe everywhere” |
| Airplane-mode / capture evidence of local Analyze | No future/compromised OS can ever exfiltrate |
| Ranks candidates vs a small current public package | Identifies, authenticates the file, locates someone, or is for enforcement. Empty ≠ clearance. Missing ≠ accusation. |
| Named models produced published metrics on disclosed fixtures | Real-world accuracy outside those fixtures |
| Matching off when the FBI cache is missing, expired, or the stored body hash mismatches; model-file hashes match git / the GitHub release | Unofficial builds or a compromised OS still enforce those checks |
| Specifically published independent review(s) | A general audit, biometric certification, or lawful deployment everywhere |

Missing boxes are release limitations — not hidden behind “open source.”
