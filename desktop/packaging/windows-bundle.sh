#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Collect a runnable Windows folder from an MSYS2 UCRT64 build. Not tested on Windows yet.
# Usage, from the repository root in a UCRT64 shell: desktop/packaging/windows-bundle.sh dist
set -euo pipefail
set -x  # Packaging failures here are otherwise opaque; trace every command in CI.

out=${1:?usage: windows-bundle.sh OUT_DIR}
prefix=/ucrt64
exe=desktop/target/release/openworld-desktop.exe
ort=third_party/onnxruntime/windows-x86_64

# Copy into $out every UCRT64-prefix DLL that $1 loads, directly or transitively.
# ntldd prints resolved Windows paths (often backslashed), so normalise the slashes
# before matching, and tolerate a binary that pulls in nothing from the prefix (an
# empty grep under `pipefail` would otherwise abort the whole script with no output).
copy_prefix_deps() {
  ntldd -R "$1" | tr '\\' '/' \
    | { grep -io "$prefix/bin/[^ ]*\.dll" || true; } | sort -u | while read -r dll; do
    cp -n "$dll" "$out/"
  done
}

test -f "$exe" || { echo "build first: cargo build --release --manifest-path desktop/Cargo.toml" >&2; exit 1; }
test -f "$ort/onnxruntime.dll" || { echo "fetch first: python scripts/fetch_ort.py windows-x86_64" >&2; exit 1; }

mkdir -p "$out"
cp "$exe" "$prefix/bin/ffmpeg.exe" "$prefix/bin/ffprobe.exe" "$out/"
cp "$ort"/*.dll "$out/"

# Every DLL the program and FFmpeg load from the UCRT64 prefix.
for binary in "$out/openworld-desktop.exe" "$out/ffmpeg.exe" "$out/ffprobe.exe"; do
  copy_prefix_deps "$binary"
done

# Bundle manifests and curves only. Weights are fetched separately.
for bundle in bundles/*/; do
  mkdir -p "$out/$bundle"
  cp "$bundle/manifest.toml" "$out/$bundle"
  if [ -f "$bundle/curve.json" ]; then cp "$bundle/curve.json" "$out/$bundle"; fi
done

# GTK's run-time data: settings schemas, icons, and image loaders.
mkdir -p "$out/share/glib-2.0/schemas" "$out/share/icons" "$out/lib"
cp "$prefix"/share/glib-2.0/schemas/*.xml "$out/share/glib-2.0/schemas/"
glib-compile-schemas "$out/share/glib-2.0/schemas"
cp -r "$prefix/share/icons/Adwaita" "$prefix/share/icons/hicolor" "$out/share/icons/"
cp -r "$prefix/lib/gdk-pixbuf-2.0" "$out/lib/"
for dll in "$out"/lib/gdk-pixbuf-2.0/2.10.0/loaders/*.dll; do
  copy_prefix_deps "$dll"
done
GDK_PIXBUF_MODULEDIR="$out/lib/gdk-pixbuf-2.0/2.10.0/loaders" gdk-pixbuf-query-loaders \
  | sed "s|$(cygpath -m "$out")/||g" > "$out/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"

# The license files of every MSYS2 package a copied file came from, and the package list.
mkdir -p "$out/share/licenses"
find "$out" -type f \( -name '*.dll' -o -name '*.exe' \) ! -name openworld-desktop.exe -printf '%f\n' | sort -u | while read -r leaf; do
  if [ -f "$prefix/bin/$leaf" ]; then pacman -Qqo "$prefix/bin/$leaf" 2>/dev/null || true; fi
done | sort -u > "$out/share/licenses/packages.txt"
while read -r package; do
  pacman -Ql "$package" | awk '{print $2}' | { grep "^$prefix/share/licenses/." || true; } | while read -r file; do
    if [ -f "$file" ]; then
      mkdir -p "$out/share/licenses/$(dirname "${file#"$prefix"/share/licenses/}")"
      cp "$file" "$out/share/licenses/${file#"$prefix"/share/licenses/}"
    fi
  done
done < "$out/share/licenses/packages.txt"
cp docs/legal/third-party.txt "$out/share/licenses/openworld-third-party.txt"

echo "bundled into $out"
