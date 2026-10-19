# Packaging the desktop app

## Linux: Flatpak

`app.openworld.OpenWorld.json` builds the app on the GNOME 46 runtime, with:

- FFmpeg built from its tagged source as a module, because the core runs the `ffmpeg` and `ffprobe` programs. The freedesktop `ffmpeg-full` extension ships only libraries, not those programs. The module turns off autodetected external libraries, so it decodes with FFmpeg's own decoders (no AV1 without dav1d).
- ONNX Runtime from the same PyPI wheel `scripts/fetch_ort.py` pins, checked by that SHA-256 (`scripts/test_fetch_ort.py` checks that this manifest and `core/openworld-core/onnxruntime.pins` agree). The core checks the installed `/app/lib/libonnxruntime.so` against its library pin before loading it.
- The bundle manifests (and curves) beside the program in `/app/bin/bundles`. Weights are fetched with `scripts/fetch_weights.py`, never committed.

Cargo runs offline inside flatpak-builder, so the crates are listed first:

```sh
python3 flatpak-cargo-generator.py desktop/Cargo.lock -o desktop/packaging/cargo-sources.json
flatpak-builder --user --install --force-clean build-dir desktop/packaging/app.openworld.OpenWorld.json
flatpak run app.openworld.OpenWorld
```

`flatpak-cargo-generator.py` is in `flatpak/flatpak-builder-tools`. The generated file is not committed. The `dir` source is for local builds; a Flathub build names a tagged commit of this repository instead. The `rust-stable` extension for the GNOME 46 runtime (freedesktop 23.08) may be older than the Rust that `gtk4` 0.11 needs (1.92); a newer GNOME runtime fixes that.

Results and the demo posters live in the sandbox's data folder (`~/.var/app/app.openworld.OpenWorld/data/openworld`).

## Windows

See `windows.md`.
