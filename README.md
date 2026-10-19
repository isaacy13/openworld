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
| **Fast** (selected) | YuNet 2023mar (MIT) + SFace 2021dec (Apache-2.0) | 0.63, above the measured cutoff of 0.5923 | [`bundles/fast/curve.json`](bundles/fast/curve.json): 65 false matches in 81 million LFW pairs (8.0e-7). It misses 13% of LFW same-person pairs and 73% of cross-age pairs (CALFW). |
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

There is no release yet, so there is no digest to print here. This README does not invent one.

A release is built by [`.github/workflows/release.yml`](.github/workflows/release.yml) from a `v*` tag. Each file on the release page has a build-provenance attestation, and the page lists the files in `SHA256SUMS`. Before you run a file:

1. Check where it was built. With the [GitHub CLI](https://cli.github.com/), `gh attestation verify <file> --repo isaacy13/openworld` must say the file was built by this repository's release workflow, and name the tag and commit. If it fails, do not run the file.
2. Check its SHA-256. Run `sha256sum <file>` (on macOS, `shasum -a 256 <file>`; on Windows, `Get-FileHash <file>`), and compare the result with the digest GitHub shows for that file on the release page, and with the digest this README will give for the current release. `SHA256SUMS` lists the same digests and is attested too. A checksum file that ships beside a binary is not the check on its own.
3. Check the signature. No release signing key is pinned yet (see [keys/](keys/README.md)), so releases are not signed with one. Files named `-unsigned` were built without the owner's signing key.

The archive holds the program, the ONNX Runtime library it loads (the core refuses any library whose SHA-256 is not the pin in [`core/openworld-core/onnxruntime.pins`](core/openworld-core/onnxruntime.pins)), and the bundle catalog. Weights are fetched with `scripts/fetch_weights.py`, and their SHA-256 is pinned in each bundle's manifest. CycloneDX SBOMs (`*.cdx.json`) list the Rust crates and the Gradle artifacts each build used.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Apache-2.0; see [LICENSE](LICENSE).
