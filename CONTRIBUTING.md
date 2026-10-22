# Contributing

Thank you for helping. Read [AGENTS.md](AGENTS.md) and [docs/decisions.md](docs/decisions.md) first: the product limits there (import only, on-device, no auto-report, no clearance language, plates out of v0) are not open for change in a pull request.

## Ground rules

- **No photographs of real people in the repository**, in tests, fixtures, or screenshots. Use the demo markers (`openworld fixture-still`) or synthetic files. Public face sets are downloaded by `eval/` and never committed.
- **No training** on user media or poster photos.
- **No invented hashes.** A SHA-256 comes from the file or from its publisher.
- **No English-text logic.** Shells decide by codes and statuses; words come from the core as Lines.

## Build and test

```sh
python3 scripts/fetch_ort.py          # the pinned ONNX Runtime the core loads at run time
python3 scripts/fetch_weights.py fast # optional: the Fast weights, kept only if the SHA-256 matches
cargo test --manifest-path core/Cargo.toml
cargo clippy --manifest-path core/Cargo.toml -p openworld-core -p openworld-cli -- -D warnings
```

The desktop app is in `desktop/` (`cargo run --manifest-path desktop/Cargo.toml`, needs GTK 4 and libadwaita), Android in `android/` (`./gradlew assembleDebug testDebugUnitTest`; `./gradlew connectedDebugAndroidTest` on an emulator or device runs the instrumented scan and video tests), and the Apple apps in `apple/` (on a Mac: `apple/scripts/build-core.sh`, then `xcodegen generate` in `apple/`; the `OpenWorldTests` scheme runs the kit and decoder tests on an iOS simulator, `OpenWorldMac` on the Mac; `swift test --package-path apple` runs the kit's tests anywhere).

## Pull requests

- Keep each change small and focused, with tests for new behavior and for any bug you fix.
- Match the surrounding code: short plain comments, clear names.
- Write commit messages that say what changed and why, in a sentence.
- CI must be green: the core tests and clippy, the self-test, the sampled real-photo check, and the shells that your change touches.
- A new or changed `uses:` in a workflow: run `python3 scripts/pin_actions.py` to pin it to a commit SHA. CI runs `--check`.
- A new or changed Android dependency or plugin: the CI android job regenerates `android/gradle/verification-metadata.xml` and fails with the new file when it differs. Review it and commit it. Do not edit hashes by hand.
- A new ONNX Runtime: change `core/openworld-core/onnxruntime.pins` only, with digests from PyPI and from the files in the checked wheels (`python3 scripts/fetch_ort.py --all` re-checks every one).

Security issues go through [SECURITY.md](SECURITY.md), not public issues.
