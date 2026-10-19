# OpenWorld library API

The shells call one library, `openworld-core`, through `include/openworld.h` (Swift, Kotlin via JNI) or directly (the Rust desktop app). They never detect, track, compare, or decide what a result means. Everything below is JSON.

## Lines

Every sentence a person reads is a **Line**:

```json
{"code": "faces_some", "text": "3 faces found. 2 could be compared. The rest are not cleared.", "args": {"seen": 3, "compared": 2}}
```

Show `text`, or translate by `code` with `args`. Never compare `text` to decide anything; read codes and statuses. `{"v":1,"op":"copy"}` returns every fixed sentence by code, for building a string catalog.

## Calls: `ow_call(request, on_event, user)`

Every request has `"v": 1` and an `"op"`. Unknown fields are refused. Every answer has `"ok"`; a refusal is `{"ok": false, "refusal": "<code>", "headline": Line}`.

| op | Fields | Answer |
| --- | --- | --- |
| `version` | | `api`, `library`, `demo_posters` (bool), `providers` |
| `bundles` | `bundles` (catalog folder) | `bundles`: rows `{id, name, best_for, preselected, curve_exists, curve_line: Line, real_posters_allowed, weights_ready, official}`, `note` |
| `estimate` | a scan request | `human` Line, `seconds`, `frames_analyzed`, `detection_long_side`, `detection_line`, `coverage_line`, `coverage_banner?`, `notes: [Line]`, `suggest_computer`, `caveat`, `warnings: [Line]` about the file (old, timestamps disagree), `disclosure: [Line]` to show before Analyze, `classes` Line, `can_start` |
| `scan` | a scan request | the report (desktop only: the library decodes the file with FFmpeg) |
| `posters_check` | `posters` | `posters: {id, source, sha256, created_at, expires_at, missing, wanted, demo}` |
| `posters_demo` | `out` | writes the demo pack (demo builds) |
| `posters_update` | `bundles`, `bundle` (default `"fast"`), `posters` (the pack folder) | `posters` (as `posters_check`), `kept`, `skipped: [{reason: Line, count}]`, `list_sha256`. See [Update posters](#update-posters). Refuses `real_posters_off` until the selected bundle is official and its curve allows real posters |
| `leave` | `url` | `{message: Line, url}`; refuses `only_fbi` for anything but an `https://www.fbi.gov/...` page |
| `delete` | `out` | refuses `delete_foreign` for a folder that is not an OpenWorld result |
| `sweep` | `root` | `removed`: expired results deleted under `root` |
| `copy` | | `copy`: `{code: text}`, `disclosure: [Line]` for a report the shell makes itself (a scan the system killed), without the clearance line |
| `legal` | | `version` (the terms version to accept), `contact`, `first_run: [Line]`, `accept` Line, `changed` Line (shown instead of nothing when an older version was accepted), `documents: [{id, title: Line}]`. See [First run](#first-run) |
| `legal_document` | `id` (`terms`, `privacy`, `biometric`, `health_data`, `licenses`) | `{id, title: Line, paragraphs: [String]}`, the built-in text as plain paragraphs; refuses `no_document` |

### Scan request

```json
{
  "v": 1, "op": "scan",
  "input": "/path/to/file",        // the file; a phone that decodes it still names it
  "bundles": "/app/bundles", "bundle": "fast",
  "posters": "/app/posters",
  "out": "/app/results/2026-10-19T10-00-00Z",   // new or empty; never a folder of yours
  "coverage": "complete",          // or "measured"
  "missing": true, "wanted": true,
  "accelerate": true,              // try the GPU or neural engine first
  "device": {"kind": "phone", "thermal": "nominal", "battery": 0.8, "charging": false},
  "file_mtime_unix": 1760000000,   // the original's modified time, when you imported a copy
  "media": { ... }                 // only for a pushed scan, below
}
```

## Pushed scans (phones)

The phone decodes with AVFoundation or MediaCodec and pushes frames:

1. `ow_job_start(request, on_event, user, &refusal)` with a scan request (`"op": "scan"` may be left out) and `media: {width, height, frames, fps, duration_sec, video, container_unix}`. `frames` is how many pictures you will deliver; a different count is Incomplete. Several pictures that are not a video (a multi-page TIFF, a multi-image HEIC) are pages: `video: false`, `fps: 1`, and each page decides alone, like a still. On refusal it returns NULL and `refusal` holds `{ok:false, refusal, headline}`.
2. For each decoded frame `i` in order: if `ow_job_wants(job, i)` is 0, call `ow_job_skip(job, i)` without converting the frame. Otherwise push it with `ow_job_push_yuv420` (MediaCodec `Image` planes, CVPixelBuffer 420 planes) or `ow_job_push_pixels` (RGBA, BGRA, RGB), with the rotation that shows it upright. A push returns 0 when the scan stopped: finish then.
3. `ow_job_stop_token(job)` gives a token any thread can use: `ow_stop(token, "cancel" | "heat" | "battery" | "background")`. Watch `ProcessInfo.thermalState` / `PowerManager.getCurrentThermalStatus()`, battery level, and the app leaving the foreground.
4. `ow_job_finish(job, "{\"clean\": true}")` when the decoder reached the end without an error (`false` otherwise). It returns the report and frees the job.

Push from one thread. Events arrive on that thread; hop to the UI thread to show them.

## Events

JSON lines to `on_event`:

```json
{"event": "face", "track_id": 3, "frame": 41, "frame_label": Line, "crop": "crops/face_3.jpg"}
{"event": "track", "track": TrackItem}
{"event": "progress", "frames_analyzed": 450, "frame": 899}
```

`crop` paths are relative to `out`. A `face` event is a first look; the `track` event replaces it with the decided row.

`posters_update` sends its own progress:

```json
{"event": "posters_page", "page": 3, "pages": 26}
{"event": "posters_checked", "checked": 120, "posters": 900}
```

`pages` is `null` when the list does not give a total. `posters_checked` comes every 10 photos and after the last.

## First run

Before the first scan, a shell shows the `legal` answer's `first_run` lines, its `accept` line, a link to each document, and one **Continue**. Continue stores `version` in the shell's own preferences; nothing is sent. At each launch the shell compares the stored value with `version`: equal opens the app; anything else shows the screen again, with `changed` above it when some version was accepted before. The documents are in the library (`docs/legal/`, and `docs/legal/third-party.txt` from `scripts/notices.py`), so reading them uses no network. About shows the same links and `contact` at any time.

## Update posters

`posters_update` reads every page of `https://api.fbi.gov/wanted/v1/list` (`page`, `pageSize` 50, until its `total`), keeps the open, named missing and wanted posters ([record classes](record-classes.md)), downloads each one's lead photo (`original`, else `large`), and builds its vector on this device with the selected bundle's detector and embedder.

- Only `https://` addresses on `api.fbi.gov`, `www.fbi.gov`, and `fbi.gov` are requested, and a redirect is followed only to one of them. The request names OpenWorld in its User-Agent and carries no cookies or other headers. A list page is capped at 8 MB and a photo at 20 MB; each request has a 60-second deadline, with a pause between requests.
- Each response body is hashed; headers are never an input. `list_sha256` is the SHA-256 of the pages' body hashes, one per line, in page order. The pack id is `fbi-` and its first 16 hex digits.
- A poster gets a vector only when its photo is a JPEG, PNG, or WebP under the picture limit and shows **exactly one face** that passes the quality gates at the compare size. Every box the detector reports counts, however small or unsure. Any other poster is left out and counted in `skipped` with its reason: `skip_not_open`, `skip_no_photo`, `skip_photo_unavailable`, `skip_photo_too_large`, `skip_not_picture`, `skip_no_face`, `skip_several_faces`, a gate's `why_*`, or `face_unscored`.
- The pack is written beside `posters` and moved into place only after it loads and checks. Its photos sit beside the snapshot, pinned by SHA-256; only kept posters' photos are stored. `created_at` is the fetch time and `expires_at` seven days later.
- `posters` must be new, empty, or a poster pack; anything else refuses `posters_foreign`. A failed update leaves the old pack as it was and refuses with `posters_fetch_failed`, `posters_bad_list`, `posters_runtime`, `posters_none` (no poster passed), `posters_unwritten`, or `network_off` (a build without the `network` feature).

## The report (`openworld.result.v2`)

| Field | Meaning |
| --- | --- |
| `status` | `complete`, `incomplete`, or `refused`. Only `complete` with no candidates may show "No candidate is not a clearance." |
| `headline` | The Line for the headline card. |
| `reason` | Why it is incomplete. |
| `refusal` | The refusal code. |
| `candidates` | Cards: `{headline, track_id, frame_index, frame_label, band, poster: {id, class, class_label, title, fbi_url?, image?}, crop, frame, leaving, leave_notes: [Line]}`. `poster.image` is an absolute path to the poster photo. |
| `tracks` | One row per face: `{track_id, first_frame, last_frame, frames: Line, face_px, quality_frames, status, label, reason?, crop}`. `status`: `candidate`, `below_cutoff`, `too_brief`, `not_compared`, `unscored`. |
| `counts` | `frames_decoded`, `frames_analyzed`, `frames_declared?`, `faces_seen`, `faces_compared`, `faces_not_compared`, `candidates`. |
| `frames_line`, `faces_line`, `classes`, `coverage_line`, `coverage_banner?`, `detection_line`, `device_note?`, `chance?`, `perception_note?` | Lines for the details section. `perception_note` marks demo posters. |
| `warnings`, `disclosure` | Lines. The disclosure leaves out the clearance line unless the scan is complete. |
| `bundle`, `posters` | Provenance: names, versions, SHA-256s. |
| `created_at`, `expires_at` | The result is swept after `expires_at`. |

There are no scores in a shipped build.
