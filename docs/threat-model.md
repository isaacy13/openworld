# Threat model

What OpenWorld protects, from whom, and how. Scope: the v0 apps (import-only, on-device, no server) and the release process.

## Assets

| Asset | Why it matters |
| --- | --- |
| The user's photo or video | Often private. May show people who never agreed to be scanned. |
| Face crops, templates, and results | Biometric data of bystanders. |
| The poster pack | Decides who can become a candidate. A swapped pack is a watchlist. |
| The bundle weights and cutoff | Decide how often strangers become candidates. |
| A person on a poster | A false candidate can lead to fear, a false report, or confrontation. |
| The person in the file | A false candidate is an accusation by a machine. |

## Adversaries and what stops them

| Threat | Mitigation |
| --- | --- |
| **Uploading or leaking media.** | No network code in the scan path. The only network action is a separate, explicit Update posters, which stays off. Android requests no INTERNET permission for scans; iOS and Mac have no network entitlement need for a scan. No telemetry. |
| **Using OpenWorld as a watchlist** (load your own photos as posters). | The shipped CLI and apps cannot turn a photo into a poster. A pack is bound to the embedder that made it. Official packs come only from the FBI list, when allowed. A user who edits files on their own device is out of scope (the models are open; they do not need our app). |
| **Threshold shopping** (lowering the cutoff). | The cutoff is in the bundle manifest, which is pinned into the official build. There is no slider, no raw score, and no near-miss list. |
| **A spoofed official bundle** (a folder that says `official = true`). | Official manifests and curves are pinned by SHA-256 at build time. A folder must be named for its id; weight paths stay inside the folder; weights load from the bytes that were hashed. |
| **A tampered poster pack** (scaled vectors, swapped photos). | The snapshot is hashed; each poster photo is pinned inside it; vectors must be unit length and the embedder's size; packs older than 7 days or dated in the future refuse. |
| **Malicious media** (decompression bombs, malformed containers, argument injection). | Pictures over 120 MP refuse before decoding; frames are streamed one at a time; container headers are read by seeking with fixed bounds; parsers are iterative with depth limits; FFmpeg reads only local `file:` inputs with a protocol whitelist; FFmpeg's error pipe is drained; no panic crosses the FFI boundary. |
| **Destroying user files** through a crafted result path. | A scan only writes to a new or empty folder or one it marked; it refuses a folder that holds the input, the posters, or the bundles; delete and sweep touch only marked folders. |
| **Overstating coverage** (a partial scan read as "no match"). | Complete needs a clean decode and an agreeing frame count. Every stop has a reason and makes the scan Incomplete. The clearance sentence is never shown on an incomplete scan. |
| **A compromised build or dependency.** | ONNX Runtime and the weights are pinned by SHA-256. CI actions run with read-only tokens. Planned: SHA-pinned actions, signed release artifacts, an SBOM, cargo-deny in CI. |
| **A compromised FBI origin.** | Out of scope for a checksum. The pack records what was fetched and when; a later on-disk edit fails the hash. |

## Trust boundaries

1. The file on disk → the decoder (untrusted input).
2. The poster pack and bundle folders → the core (checked against pins and hashes).
3. The core → the shell (typed JSON; the shell shows Lines and never decides meaning).
4. The shell → the browser (only the poster's own `https://www.fbi.gov/...` page, after a confirmation sheet).

## Out of scope

A compromised device, other apps, screenshots, and a user who modifies their own build. On-device processing is privacy, not proof the file is real.
