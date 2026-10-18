# Platforms

One app on different hardware. Performance and efficiency are designed in. The UI is fluid and follows each platform’s design language. The same flows and the same words make it obviously the same app.

## One core

Behavior is one Rust core — hash check, coverage, gates, tracker, cosine — plus the bundle folder the user picked. UI shells call that core. They do not reimplement the scan. Coverage and gates stay in [project context](project-context.md).

A bundle is a folder: manifest, ONNX files, that bundle’s locked threshold, and a one-line **best for**. The core loads any bundle the manifest describes. Add or remove a model by adding or removing a folder. No scan-code change. The pick is the whole folder, not files mixed from two bundles.

Before Analyze, the user picks a bundle. The row shows the name, the best-for line, and whether an FMR/FNMR curve exists. The time estimate matches that pick. Scores are not comparable across bundles. Results name the bundle. The official UI does not load a random ONNX from disk. Unofficial or developer manifests stay labeled unofficial.

v0 starts with one recommended mix, pre-selected: **Fast** — SCRFD-0.5GF + small ArcFace. Best for phones and long video. The catalog can list other bundles aimed at other goals, including a larger slot. v0 also lists **Accurate** as an addable slot, not a silent swap: larger SCRFD + larger ArcFace (ResNet-100 class). Best for a computer when they want fewer misses. It is selectable. If its curve does not exist yet, the row says **not measured yet**. That best-for line is the goal of the slot. Do not describe it as more accurate until the curve exists. Plates stay inside each bundle (nano detector + crop OCR), not a separate face model. A later OCR bundle is another folder.

## Shells

| Where | UI | Design language |
| --- | --- | --- |
| iPhone, Mac | SwiftUI | Apple HIG |
| Android | Kotlin / Jetpack | Material |
| Windows, Linux | Native desktop window | That platform’s windows and type |

Same flows and the same words for disclosure, estimates, the downscale prompt, candidates, and the FBI link. Controls, navigation, and typography follow the platform. Disclosure words: [trust](trust.md). Estimates and the downscale prompt: [intake](frictionless-ux.md).

## v0 design bar

v0 screens are part of the product, not a debug UI left for a later polish pass. iPhone and Mac use Apple HIG and SwiftUI. Android uses Material. Windows and Linux use that platform’s normal window, type, and spacing. Same flows and the same words. Layout, controls, and motion differ, so it feels like a well-made app from that platform. Intuitive means the next action is obvious: pick media, see the disclosure, pick the bundle and detection size, see the estimate, start, watch progress, review crops, open the FBI link or delete. Beauty is native materials, clear type, and lots of space, not a dense engineer dashboard.

## Performance

- Platform decoder: AVFoundation on Apple, MediaCodec on Android, FFmpeg on Windows and Linux.
- One in-memory resize. No extra frame copies.
- ONNX Runtime on the provider that loads.
- Embeddings and OCR only on hits.
- One pipeline. No second scan per OS.

## Still open

Nothing else has to be decided before the first code. These stay open, and they are not blockers:

- GitHub auth for [isaacy13/openworld](https://github.com/isaacy13/openworld) (the repo is empty).
- Exact ONNX filenames, pinned at the first commit.
- The 1-second video count for the accuracy curve ([validation](validation.md)).
