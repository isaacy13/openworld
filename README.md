# OpenWorld

OneX Engineering will successfully privatize open source world peace.

OpenWorld checks a photo or video you already have against public FBI missing-person and wanted-person posters, **on the device where the file is**. It finds the faces, follows them through a video, and compares each one to a small, frozen poster pack with a locked cutoff. A face that passes is shown as **"Possible candidate. Not an identification."**: the crop from your file beside the poster photo, the frame it came from, and how far past the cutoff it was. One tap, through a confirmation, opens that poster's own FBI page. OpenWorld never sends a report.

> **Status: pre-release.** There is no release yet. **Real FBI posters stay off**: the apps scan against a demo poster pack of synthetic markers until the cutoff is measured on photos like the posters. See [docs/decisions.md](docs/decisions.md).

## What it is not

Import only: no camera, no watch folder, no background scan. Nothing is uploaded, nobody is enrolled, and nothing trains on your file. A candidate is not an identification. **No candidate is not a clearance.** On-device does not mean the file is real. Not NCIC, NCMEC, NamUs, INTERPOL, or an Amber alert.

## How it works

```text
your file → decode one frame at a time (FFmpeg on a computer; AVFoundation / MediaCodec on a phone)
          → YuNet finds faces at a size chosen so a 112 px face is found
          → the tracker follows each face
          → hard quality gates: size, landmarks, confidence, pose, blur, exposure
          → the 5 best frames of each face are embedded once by SFace and averaged
          → locked cosine against the poster pack
          → a video face is a candidate only after 3 gated frames or 1 second
          → one row per face; cards for candidates; nothing leaves the device
```

One Rust library (`core/`) does all of it. The apps only show what it returns ([docs/api.md](docs/api.md)).

| Bundle | Models | Cutoff | Measured |
| --- | --- | --- | --- |
| **Fast** (selected) | YuNet 2023mar (MIT) + SFace 2021dec (Apache-2.0) | 0.63: false match rate at or under one in a million on LFW | `eval/real_faces.py`; the result reads "Not measured yet" until `bundles/fast/curve.json` is published for this pipeline |
| **Accurate** | Larger SCRFD + ResNet-100 ArcFace (no redistributable weights yet) | 0.60 | Not measured yet |

Every result says how many false candidates chance alone would give that scan. With *f* the false match rate, *N* posters, and *M* faces, that is about *f·N·M*. It is why the cutoff is strict.

## Try it

You need Rust, Python 3, and FFmpeg.

```sh
python3 scripts/fetch_ort.py              # pinned ONNX Runtime, checked by SHA-256
cargo build --release --manifest-path core/Cargo.toml -p openworld-cli
core/target/release/openworld demo --out /tmp/openworld-demo
```

That writes a demo poster pack and a synthetic still, scans it, and prints the result. `openworld analyze` walks through the same steps for a file you choose, with the disclosure and the estimate, and asks before it starts.

A demo pack is read with the demo markers, not the face models. The face models run against a pack of real face vectors, and the only such packs today are the ones `eval/` builds from public face sets for measurement (`python3 scripts/fetch_weights.py fast` fetches the weights). The shipped commands cannot turn a photo into a poster.

`openworld --help` lists every command. `--json` prints the library's own answer. `openworld bench` times the models on this device and checks that an accelerated provider scores like the CPU.

## Apps

| Platform | Folder | Built with |
| --- | --- | --- |
| Linux, Windows | `desktop/` | Rust, GTK 4, libadwaita; links the core |
| Android | `android/` | Kotlin, Jetpack Compose, Material 3; MediaCodec; the core through JNI |
| iPhone, Mac | `apple/` | SwiftUI; AVFoundation; the core as an XCFramework |

All three follow one design: [docs/ux.md](docs/ux.md).

## Measurement

`eval/` measures Fast on public face sets (LFW, CALFW, CPLFW, WIDER FACE) that are downloaded, pinned by SHA-256, and never committed. LFW is celebrity press photos, not mugshots or posters, and has no demographic labels, so it cannot by itself turn real posters on. See [eval/README.md](eval/README.md).

## Trust

- [docs/threat-model.md](docs/threat-model.md): assets, adversaries, mitigations.
- [docs/data-flow.md](docs/data-flow.md): every piece of data and when it is deleted.
- [SECURITY.md](SECURITY.md): reporting a vulnerability.
- The design specs are in [docs/](docs/), starting with [docs/handoff.md](docs/handoff.md) and [docs/decisions.md](docs/decisions.md).

## Checking a release

There is no release yet. When one exists, compare the file you downloaded with the SHA-256 published on that GitHub release, then check its signature against the public key pinned in [keys/](keys/). A checksum shipped next to a binary is not the check. This README does not invent a digest.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Apache-2.0; see [LICENSE](LICENSE).
