# Frictionless, deliberate intake

Make *choosing media* quick. Do not make analysis invisible. v0: user hands the app a photo/video, sees what is running, and can stop or delete it. No hidden camera/mic, no silent upload, no public-space scanner.

## Before work starts

- **What:** filename/thumbnail, duration, size, source, coverage (`complete`|`measured`), and a detection-size choice with the time estimate for that choice. On a phone, also heat and battery.
- **Where:** “on this device”; any network action shown separately.
- **When:** persistent status + progress; Pause, Cancel, Delete.
- **Next:** results expire with the package or in 7 days. Never auto-report. Empty ≠ clearance.

Also show the [trust](trust.md) disclosure (crops/templates, inspectable `missing`/`wanted` counts, coverage, no upload/train/enroll/auto-report, file unauthenticated, on-device ≠ proof). Coverage/preflight: [design gaps](design-gaps.md). “Easy” never means turning camera, library, or network into a silent sensor.

## v0

**Desktop** (suggested for long files): drag-and-drop and a file picker into a **queue** — drop does not start Analyze. Queue shows duration, the time estimate for the detection size they picked, named coverage, that resolution, and disk impact. The user starts `complete`, or explicitly chooses warned `measured`. Heat/OS **pauses**; a paused or partial job is not `complete`. Coverage ledger of media-time actually sent to the detector. Incomplete ≠ no-candidates. Deletable inputs/crops/templates/results.

**Phone:** system picker (least library permission) and share-sheet “Open in OpenWorld” (one item, same confirmation). No duration or size cap. Before Analyze, show time, heat, and battery for the detection size they picked. Suggest a computer when the estimate is long. The user may still run `complete`. Never silently switch that choice to `measured`. Foreground-only; OS kill → **incomplete**. Share-in is not permission to watch that app later.

**Detection size.** Before Analyze, ask: smaller frame (about 320–640 px on the long side; faster, misses small faces) or full resolution (slower). No silent default. The resize is in memory, not a re-encode, and it is cheap next to the detector. Show the time estimate for the option they pick. **64 px** short side on the **detection** image means the detector saw a face; comparison needs about **112 px** on the original crop. Evidence crops still come from the original frame when available.

One Rust core loads the bundle the user picks before Analyze: Fast is the default, and Accurate is selectable and labeled **not measured yet** until its curve exists. Warn from the provider ORT loaded, not a hardware inventory. iOS: A11 and later have the Neural Engine; warn when Core ML / ORT falls off the ANE. Android has no reliable has-NPU bit: neural shows no warning; GPU gets a short note; CPU gets a pre-Analyze heads-up (slower, warmer, more battery) and the scan stays allowed. Computer: Apple Neural Engine or GPU on Mac; CUDA, DirectML, or a vendor NPU (Intel/AMD/Qualcomm) on Windows when it loads; else CPU. Warn only when the loaded path is CPU; a shorter note if GPU is clearly slower. A long estimate suggests a computer because of cooling and power, not because phones lack a neural chip. The user can still run `complete` on the phone. Heat still pauses, and a paused job is not `complete`.

## Later (not v0)

| When | Pattern | Guardrail |
| --- | --- | --- |
| Later | In-app recorder, then Analyze / Keep / Delete (default: no analysis) | Needs camera permission; not a shortcut around import |
| Later, opt-in | Named watch folder or album as a **queue** | User picks the exact source; Analyze still explicit; never whole disk/library; notify “ready to review,” not a match |
| Later, careful | Live camera with giant preview, non-hideable “LOCAL ANALYSIS ACTIVE,” obvious Stop, no background | Easy to read as surveillance |

If a recorder or live camera happens later: foreground only; preview on the main screen; indicator that cannot hide; no mic unless a separately described audio feature (audio matching still out); visible Stop + delete; no lock-screen/auto-start/auto-report.

## Intake is ready when a tester can show

Selecting a file does not start Analyze before disclosure. Detection size is a prompt, and the estimate matches that choice. A long or large phone file can still be `complete` if the user accepts the estimate and every frame is sent. `complete` is never silently sampled or switched to `measured`. Every scan has status, coverage ledger, cancel, and local delete. Airplane mode still analyzes imported media; no media-derived telemetry. Phone reports thermal/OS interruption instead of overstating coverage. Accelerator copy follows the loaded provider on phone and computer (CPU heads-up; short GPU note, and on a computer only when GPU is clearly slower; no warning on a neural chip or the ANE) and still allows Analyze. Share/folder/album grants only what the user chose. Candidates are review items, not IDs, certificates, or alerts. Empty/incomplete ≠ clearance.
