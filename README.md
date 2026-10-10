# OpenWorld

OpenWorld looks at a photo or video you already have, on the phone or computer where the file is. It compares faces, and plates only in a narrow case, to a small frozen set of public FBI missing-person and wanted-person posters. If a score passes that bundle's locked cutoff, you see the frame, the crop, the poster, and the uncertainty on that candidate's card. The face card states the score and the cutoff for the bundle you picked. That score line does not repeat "Possible candidate. Not an identification." The plate card states the plate text that matched, and it does not repeat that sentence either. Other crops from the file are listed under the cards. A tap asks to open that poster's FBI page. The screen says "You are leaving OpenWorld." OpenWorld only opens an FBI page. The app does not send a report. A completed scan with no candidate states "No candidate is not a clearance." as the headline and again in the disclosure.

v0 scans a synthetic fixture. Real FBI photos stay off.

## What this is not

Import only. There is no camera permission, no watch folder, and no background scan.

Nothing is uploaded. There is no account, no enrollment, and no training on your file or on poster photos. OpenWorld does not contact an agency. A candidate is not an identification. No candidate is not a clearance. On-device does not mean the file is real.

This is not NCIC, NCMEC, NamUs, INTERPOL, or an Amber or Silver feed. Amber is a phone emergency broadcast.

Plate text is read only when an enabled poster publishes that plate and the crop passes the quality gate. A plate whose text is not on a poster says "This plate text is not published on a poster." A plate that cannot be read says "The plate could not be read." When no enabled poster publishes a plate, the crop says "No poster publishes a plate." A vehicle is not a person.

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

Drop a photo or video, or use Choose File. Fast is selected. The bundle page lists Fast first and marks the bundle that will run. Going back to the file and pressing Continue keeps the bundle you already picked. Choose another file selects Fast again. 640 px on the long side is selected. The estimate is shown before Analyze. A one-second estimate says "About 1 second." A one-minute estimate says "About 1 minute." A file that cannot be read stays off the next page and says "The file could not be read. Refusing." The scan runs on the CPU in this build and says so. After Analyze, the screen says Scanning until the result is ready. The previous result is cleared before that wait. Each crop appears with its label while that scan runs, including a plate and a vehicle. On a phone they appear on the estimate, under Scanning. Back stays off during that wait, including on the phone. After a result, Back returns to the estimate, and Choose another file starts over. Delete removes that result. A directory that is not a result stays, the result stays on screen, and the screen says why. Choose another file and Analyze leave that folder on disk and continue, so a missing result file does not trap the next scan. After a real delete, Delete is gone. Scanning does not show Delete. It does not remove the file you picked. Choose another file also removes a result that would otherwise stay in a temporary folder. On Android, that button also removes the imported copy. Delete removes the result and keeps the copy so Analyze can run again. Closing the desktop window removes that temporary folder. A refused file does not offer Analyze. Back still works, and the next file can continue. The phone result screen has the same Delete button.

`apple/` is the SwiftUI app for iPhone and Mac. It decodes with AVFoundation. That executable is built on a macOS runner. Linux CI compiles the same screens with OpenSwiftUI. `apple/scripts/show-linux.sh` draws them in a terminal with OpenSwiftUI's stdout renderer, which is how this revision paints on Linux. That program is `OpenWorldScreens`. It is not the app, and the macOS build does not depend on OpenSwiftUI. A PNG still is copied byte for byte there so the phone model can run a fixture scan. Video still refuses without AVFoundation. `android/` is the Material app. It decodes with MediaCodec. Both call `ow_command` in the Rust library when that library is linked into the process (`include/openworld.h`). Host tests load `libopenworld_core` that way: `android/scripts/link-host-jni.sh` builds the JNI shim, and `OPENWORLD_LIB` is the same library for the Linux phone tests. That file is this machine's build. It is not an Android ABI and not an iOS archive. A store build still has to link those. Until it does, the phone starts the `openworld` program with the same arguments. They do not implement a second detector. FFmpeg runs only on the desktop file path, when frames were not already decoded.

```sh
python3 eval/measure_fast.py --bin core/target/debug/openworld --bundles bundles --out eval/curves/fast.json --check eval/curves/fast.json
```

## The scan

`core/` is the Rust library and the `openworld` CLI. A bundle is a folder under `bundles/`. Add or remove a model by adding or removing a folder.

- **Fast** is selected by default. It is declared as SCRFD-0.5GF plus a small ArcFace, best for phones and long video. InsightFace pretrained weights are non-commercial research only, so they are not in the tree and their SHA-256 is not invented here. Until those weights are pinned, a fixture pack is read with synthetic markers. The result says "Fixture markers were read." ONNX Runtime is linked. A weight file whose license is non-commercial research only is refused even if the hash matches, so the InsightFace zoo files are not an official bundle. A file that is pinned and allowed to ship is loaded as SCRFD, ArcFace, and the plate model. If that session does not load, the scan refuses. It does not say there was no candidate. An unfinished scan says "Incomplete." The reason is under that headline. It does not say there was no candidate either. A phone that cannot create a result folder says so on the estimate and does not offer Analyze. A decode that fails during the scan shows that refusal once. A session that loads and finds nothing is a completed scan, and it can say "No candidate is not a clearance."
- **Accurate** is declared as a larger SCRFD plus a ResNet-100-class ArcFace, for a computer when you want fewer misses. Selectable. The row says "Not measured yet." until its own curve exists.

A face enters the inventory at 64 px on the short side of the detection image. The size page says a face under 64 px on that image is left out. It is compared only when the crop from the original frame is at least 112 px on the short side. Below that the label is "Not compared." The size page says that too. It is not a candidate and it is not a clearance. The Linux phone preview prints that whole paragraph, then Continue. The estimate preview keeps Analyze. A result preview lists every candidate, the score, and the disclosure, then Choose another file. A face that large whose crop cannot be scored says "This face could not be scored."

`complete` analyzes every decoded frame. An animated GIF is every frame of that file. An animated PNG is every frame on the desktop scan. A phone that cannot read every frame of an animated PNG or animated WebP refuses. A face that is not on the first frame is still compared. A one-frame GIF stays one frame. A JPEG, a still WebP, a PNG, or a TIFF is scanned the way the photo is shown, including the camera orientation tag. Every page of a TIFF is scanned. A 16-bit TIFF sample is scanned from its high 8 bits. A palette TIFF, a bilevel TIFF, a CMYK TIFF, and a YCbCr TIFF are scanned as the picture they show. A multi-page TIFF whose pages are not all read is refused. Android refuses a TIFF. A video is scanned the way it is shown, including a quarter-turn display rotation, non-square pixels, and both together. A video whose name has no extension is still a video: Android, Mac, and iPhone read the file header. A HEIF or AVIF header stays a still. A JPEG whose name ends in .png is decoded as a JPEG. Those phones copy a frame when the header is a PNG. The Mac and iPhone writer uses that same display size and writes every page of a TIFF. The estimate on those devices counts the video track. Audio that continues after the pictures does not turn a finished decode into "Incomplete." An edit list that hides some samples leaves the scan on the frames a player shows, and that finished scan stays complete. Several frames of one track keep one card, the frame with the highest cosine. A plate track keeps the earliest frame of that track. The other frames stay in the inventory and in the comparison list. A frame under the cutoff stays out of the cards. `measured` analyzes 5 frames a second, plus the tracker, and shows "A brief face can be missed." That sentence appears on an estimate that can run. The result says how many frames were analyzed. One frame says "1 frame analyzed." A scan that left some decoded frames out says how many of them were analyzed, under the headline. It also repeats the detection size in the words of that button. 640 says "640 px on the long side." Full resolution says "Full resolution." There is no maximum duration. The estimate is a planning model, not a thermal table. It counts the pictures. Audio that continues after them keeps the same estimate. On a phone it also shows heat and battery. A long phone estimate suggests a computer. You can still run `complete` on the phone. If the job stops, the result is "Incomplete." and the screen says why, such as "The file was not fully decoded."

A bad hash, a bad codec, or a missing or expired poster pack is a refusal.

## Posters

The first pack is a fixture snapshot with the same schema a later update would use. `openworld posters update` refuses. It does not call api.fbi.gov. A curve that sets `real_posters_allowed` is what would open that path. The Fast fixture curve does not.

Classes are missing and wanted, on or off together as a class. The cutoff is the same for both. The estimate starts with both on and says "Missing and wanted." Turning wanted off says "Missing." and leaves that class out of the scan. The control stays named Wanted. The result names those classes again, under the headline. A candidate card names that poster as Missing or Wanted. An old file is warned on the file page and again on the estimate, before Analyze. Disagreeing timestamps are a warning under the result line, not part of it. An old file is repeated there too. With both off, Analyze stays on the page and does not run, and the screen says "Choose missing, wanted, or both." A refused estimate does not show Missing, Wanted, or Analyze. A scan that runs with both off says "No class was on." ECAP, unidentified remains, and unnamed Seeking Information are skipped by the list parser. A running job keeps the pack it loaded.

## Fast curve

`eval/curves/fast.json` is the fixture measurement on this program's preprocess: false match rate, false non-match rate, and the four comparison outcomes at the locked cutoff (true positive, false negative, false positive, true negative), each with a count and a 95% Wilson interval. Genuine trials use 16 fixture identities. Impostor trials use 160 pairs with a different probe identity every time. A face that was not compared is left out of that table. The file also has detection recall at the 64 px rule, faces seen but not compared, and the miss rate for a face visible about one second under both coverage modes. A shorter probe is included so the measured-mode warning has a number behind it.

It is not SCRFD, not LFW, and not NIST. Real FBI photos stay off.

`eval/heldout.py` runs a separate set through the same `openworld scan` command the desktop window uses. Eighty impostor stills use identities other than the two fixture posters, at placements the published curve does not use. A face under 64 px is left out. A face at 64 px is seen and labeled "Not compared." A compressed video of color bars, with no marker in it, stays a clearance. A lossless video of the fixture scene stays a candidate, with one card for the face track and one card for the plate track, and a container time that disagrees with the file time is a warning, not a refusal. A one-frame GIF stays one frame and a clearance. A three-frame GIF whose marker is only on the middle frame stays a candidate, and the scan reports three frames. An animated PNG of that same marker stays a candidate and reports three frames. A JPEG stored on its side, with the camera orientation tag that shows it upright, stays a candidate. A still WebP stored on its side, with that same tag, stays a candidate. A PNG stored on its side, with that same tag, stays a candidate. A TIFF stored on its side, with that same tag, stays a candidate. A 16-bit TIFF stored on its side, with that same tag, stays a candidate. A palette TIFF, a bilevel TIFF, a YCbCr TIFF, and a subsampled YCbCr TIFF of the upright scene stay candidates. A two-page TIFF whose marker is only on the second page stays a candidate, and the scan reports two frames. A video stored on its side, with a quarter-turn display rotation that shows it upright, stays a candidate. A video whose pixels are not square, and whose player shows the upright scene, stays a candidate. A video that is turned and stored with non-square pixels, and whose player shows the upright scene, stays a candidate. A video whose edit list hides the opening blank samples stays a candidate, and the scan stays complete. A video whose edit list hides the marker stays a clearance. A video whose audio continues after the pictures stays a candidate when every picture frame was decoded. An audio file is refused. None of these files are photographs of people. This set does not turn `real_posters_allowed` on.

## Continuous integration

GitHub Actions runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml) on every pull request and on pushes to `main`. The Ubuntu job rebuilds the Rust library, requires 90% line coverage, checks this Fast curve against a fresh run, checks the product sentences, and runs the Android phone tests. A second Ubuntu job runs the Apple contract, builds the screens with OpenSwiftUI, and runs the stdout preview once. The macOS job builds the SwiftUI app and does not depend on OpenSwiftUI. The desktop window is not launched there.

The crops in those phone tests are synthetic markers drawn for the fixture pack. They are not photographs of people, and they are not a training set. OpenWorld does not train on photos. Face photographs are not committed. InsightFace pretrained weights stay out. A photograph curve is still open, and `real_posters_allowed` stays false until that curve exists.

## A public release

These are still open. A fixture scan does not close them.

- Pin redistributable Fast weights (SCRFD-0.5GF, the small ArcFace, RTMDet-nano, and plate text) with name, version, SHA-256, and a license that allows shipping. InsightFace pretrained files stay out.
- Publish a photo FMR and FNMR curve for Fast on this preprocess. `real_posters_allowed` stays false until that curve says otherwise. `openworld posters update` keeps refusing, and it does not call api.fbi.gov.
- Link `libopenworld_core` into the iPhone and Android store binaries. Host tests already call `ow_command` in-process. The store ABIs are not built here.
- Measure phone heat and battery. The estimate remains a planning model.
- Legal review, store signing, and an independent audit.
- Pin `keys/openworld-release.pub` and publish the release file's SHA-256 on the GitHub release. This repository does not invent either value.

## Checking a release

There is no release yet. Do not trust a checksum that shipped next to a binary.

When a release exists, compare the file you downloaded to the SHA-256 digest published on that GitHub release, then check the signature against the public key pinned in this repository. No signing key is pinned yet. `keys/README.md` says why. This README does not invent a digest.

## License

Apache-2.0. See [LICENSE](LICENSE).
