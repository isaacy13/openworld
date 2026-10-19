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
| `estimate` | a scan request | `human` Line, `seconds`, `frames_analyzed`, `detection_long_side`, `detection_line`, `coverage_line`, `coverage_banner?`, `notes: [Line]`, `suggest_computer`, `caveat`, `classes` Line, `can_start` |
| `scan` | a scan request | the report (desktop only: the library decodes the file with FFmpeg) |
| `posters_check` | `posters` | `posters: {id, source, sha256, created_at, expires_at, missing, wanted, demo}` |
| `posters_demo` | `out` | writes the demo pack (demo builds) |
| `posters_update` | `bundles` | refuses `real_posters_off` until an official curve allows real posters |
| `leave` | `url` | `{message: Line, url}`; refuses `only_fbi` for anything but an `https://www.fbi.gov/...` page |
| `delete` | `out` | refuses `delete_foreign` for a folder that is not an OpenWorld result |
| `sweep` | `root` | `removed`: expired results deleted under `root` |
| `copy` | | `copy`: `{code: text}` |

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

1. `ow_job_start(request, on_event, user, &refusal)`, with `media: {width, height, frames, fps, duration_sec, video, container_unix}`. `frames` is how many pictures you will deliver; a different count is Incomplete. On refusal it returns NULL and `refusal` holds `{ok:false, refusal, headline}`.
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
