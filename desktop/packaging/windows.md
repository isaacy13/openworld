# Windows build

Not tested on Windows yet. These are the steps the app is written for; nothing here has been run on a Windows machine.

The app is the same Rust crate. GTK 4 and libadwaita come from MSYS2, FFmpeg runs as `ffmpeg.exe` / `ffprobe.exe` beside the program, and ONNX Runtime is `onnxruntime.dll` beside the program (the core looks there first, see `core/openworld-core/src/runtime.rs`).

## Build (MSYS2 UCRT64 shell)

```sh
pacman -S --needed git python \
  mingw-w64-ucrt-x86_64-toolchain mingw-w64-ucrt-x86_64-pkgconf \
  mingw-w64-ucrt-x86_64-rust \
  mingw-w64-ucrt-x86_64-gtk4 mingw-w64-ucrt-x86_64-libadwaita \
  mingw-w64-ucrt-x86_64-ffmpeg mingw-w64-ucrt-x86_64-ntldd
cargo build --release --manifest-path desktop/Cargo.toml
python scripts/fetch_ort.py windows-x86_64
```

`fetch_ort.py` keeps the wheel only when its SHA-256 matches the pin in that script, and writes `third_party/onnxruntime/windows-x86_64/onnxruntime.dll`. Use `windows-x86_64-directml` for the DirectML build (the core must then be built with its `directml` feature).

The MSYS2 `rust` package builds for `x86_64-pc-windows-gnu`, which links against the UCRT64 GTK. A rustup toolchain works too with `rustup default stable-x86_64-pc-windows-gnu` and the UCRT64 `bin` folder first on `PATH`.

## Bundle

`desktop/packaging/windows-bundle.sh dist` (run in the same shell) collects a folder that runs on a machine without MSYS2:

- `openworld-desktop.exe` and every DLL it loads from `/ucrt64/bin` (found with `ntldd -R`)
- `onnxruntime.dll` (and `onnxruntime_providers_shared.dll`) from `fetch_ort.py`
- `ffmpeg.exe`, `ffprobe.exe` and their DLLs
- `bundles/` with each bundle's `manifest.toml` (and `curve.json` when there is one). Weights are fetched with `scripts/fetch_weights.py`, never committed.
- `share/glib-2.0/schemas` (compiled), `share/icons/Adwaita` and `share/icons/hicolor`, and `lib/gdk-pixbuf-2.0` with its `loaders.cache`

Results and the demo posters go to `%LOCALAPPDATA%\openworld`. Temporary copies of files with no local path go to `%TEMP%\openworld-import-<pid>` and are removed on the next launch after two days.

## Release

There is no release yet. When there is one, its checksum comes from that GitHub release, not from a file shipped beside the program.
