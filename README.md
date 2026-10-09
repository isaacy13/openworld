# OpenWorld

OpenWorld looks at a photo or video you already have, on the phone or computer where the file is. It compares faces, and plates only in a narrow case, to a small frozen set of public FBI missing-person and wanted-person posters. If a score passes that bundle's locked cutoff, you see the frame, the crop, the poster, and the uncertainty. A tap opens that poster's FBI page. The screen says "You are leaving OpenWorld." The app does not send a report.

v0 scans a synthetic fixture. Real FBI photos stay off.

## What this is not

Import only. There is no camera permission, no watch folder, and no background scan.

Nothing is uploaded. There is no account, no enrollment, and no training on your file or on poster photos. OpenWorld does not contact an agency. A candidate is not an identification. No candidate is not a clearance. On-device does not mean the file is real.

This is not NCIC, NCMEC, NamUs, INTERPOL, or an Amber or Silver feed. Amber is a phone emergency broadcast.

Plate text is read only when an enabled poster publishes that plate and the crop passes the quality gate. A vehicle is not a person.

## Quickstart

You need a current stable Rust toolchain and, for video, FFmpeg. The desktop window needs GTK 4.

```sh
cargo test --manifest-path core/Cargo.toml
cargo run --manifest-path core/Cargo.toml -p openworld-cli -- demo --out /tmp/openworld-demo
```

That writes a synthetic still, a fixture poster pack, and a result directory. The summary is either "Possible candidate. Not an identification." or, when nothing passes, "No candidate is not a clearance." Crops are under `/tmp/openworld-demo/result/crops`.

```sh
python3 desktop/openworld_gtk.py
```

Drop a photo or video, or use Choose File. Fast is selected. 640 px on the long side is selected. The estimate is shown before Analyze. The scan runs on the CPU in this build and says so.

`apple/` is the SwiftUI app for iPhone and Mac. It decodes with AVFoundation. That executable is built on a macOS runner. Linux CI compiles the same screens with OpenSwiftUI. `apple/scripts/show-linux.sh` draws them in a terminal with OpenSwiftUI's stdout renderer, which is how this revision paints on Linux. That program is `OpenWorldScreens`. It is not the app, and the macOS build does not depend on OpenSwiftUI. A PNG still is copied byte for byte there so the phone model can run a fixture scan. Video still refuses without AVFoundation. `android/` is the Material app. It decodes with MediaCodec. Both call `ow_command` in the Rust library when that library is linked into the process (`include/openworld.h`). Until a store build links it, they start the `openworld` program with the same arguments. They do not implement a second detector. FFmpeg runs only on the desktop file path, when frames were not already decoded.

```sh
python3 eval/measure_fast.py --bin core/target/debug/openworld --bundles bundles --out eval/curves/fast.json --check eval/curves/fast.json
```

## The scan

`core/` is the Rust library and the `openworld` CLI. A bundle is a folder under `bundles/`. Add or remove a model by adding or removing a folder.

- **Fast** is selected by default. It is declared as SCRFD-0.5GF plus a small ArcFace, best for phones and long video. InsightFace pretrained weights are non-commercial research only, so they are not in the tree and their SHA-256 is not invented here. Until those weights are pinned, a fixture pack is read with synthetic markers. The result says so. ONNX Runtime is linked. A weight file whose license is non-commercial research only is refused even if the hash matches, so the InsightFace zoo files are not an official bundle. A file that is pinned and allowed to ship is loaded as SCRFD, ArcFace, and the plate model. If that session does not load, the scan refuses. It does not say there was no candidate. A session that loads and finds nothing is a completed scan, and it can say "No candidate is not a clearance."
- **Accurate** is declared as a larger SCRFD plus a ResNet-100-class ArcFace, for a computer when you want fewer misses. Selectable. The row says "Not measured yet." until its own curve exists.

A face enters the inventory at 64 px on the short side of the detection image. It is compared only when the crop from the original frame is at least 112 px on the short side. Below that the label is "Not compared." It is not a candidate and it is not a clearance.

`complete` analyzes every decoded frame. `measured` analyzes 5 frames a second, plus the tracker, and shows "A brief face can be missed." There is no maximum duration. The estimate is a planning model, not a thermal table. On a phone it also shows heat and battery. A long phone estimate suggests a computer. You can still run `complete` on the phone. If the job stops, the result is "Incomplete."

A bad hash, a bad codec, or a missing or expired poster pack is a refusal.

## Posters

The first pack is a fixture snapshot with the same schema a later update would use. `openworld posters update` refuses. It does not call api.fbi.gov. A curve that sets `real_posters_allowed` is what would open that path. The Fast fixture curve does not.

Classes are missing and wanted, on or off together as a class. The cutoff is the same for both. ECAP, unidentified remains, and unnamed Seeking Information are skipped by the list parser. A running job keeps the pack it loaded.

## Fast curve

`eval/curves/fast.json` is the fixture measurement on this program's preprocess: false match rate, false non-match rate, and the four comparison outcomes at the locked cutoff (true positive, false negative, false positive, true negative), each with a count and a 95% Wilson interval. Genuine trials use 16 fixture identities. Impostor trials use 160 pairs with a different probe identity every time. A face that was not compared is left out of that table. The file also has detection recall at the 64 px rule, faces seen but not compared, and the miss rate for a face visible about one second under both coverage modes. A shorter probe is included so the measured-mode warning has a number behind it.

It is not SCRFD, not LFW, and not NIST. Real FBI photos stay off.

## Continuous integration

GitHub Actions runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml) on every pull request and on pushes to `main`. The Ubuntu job rebuilds the Rust library, requires 90% line coverage, checks this Fast curve against a fresh run, checks the product sentences, and runs the Android phone tests. A second Ubuntu job runs the Apple contract, builds the screens with OpenSwiftUI, and runs the stdout preview once. The macOS job builds the SwiftUI app and does not depend on OpenSwiftUI. The desktop window is not launched there.

The crops in those phone tests are synthetic markers drawn for the fixture pack. They are not photographs of people, and they are not a training set. OpenWorld does not train on photos. Face photographs are not committed. InsightFace pretrained weights stay out. A photograph curve is still open, and `real_posters_allowed` stays false until that curve exists.

## A public release

These are still open. A fixture scan does not close them.

- Pin redistributable Fast weights (SCRFD-0.5GF, the small ArcFace, RTMDet-nano, and plate text) with name, version, SHA-256, and a license that allows shipping. InsightFace pretrained files stay out.
- Publish a photo FMR and FNMR curve for Fast on this preprocess. `real_posters_allowed` stays false until that curve says otherwise. `openworld posters update` keeps refusing, and it does not call api.fbi.gov.
- Link `libopenworld_core` into the iPhone and Android store binaries. The C entry is in the tree. The current phone projects still start the program when the library is absent.
- Measure phone heat and battery. The estimate remains a planning model.
- Legal review, store signing, and an independent audit.
- Pin `keys/openworld-release.pub` and publish the release file's SHA-256 on the GitHub release. This repository does not invent either value.

## Checking a release

There is no release yet. Do not trust a checksum that shipped next to a binary.

When a release exists, compare the file you downloaded to the SHA-256 digest published on that GitHub release, then check the signature against the public key pinned in this repository. No signing key is pinned yet. `keys/README.md` says why. This README does not invent a digest.

## License

Apache-2.0. See [LICENSE](LICENSE).
