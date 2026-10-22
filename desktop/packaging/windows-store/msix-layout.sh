#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Lay out an MSIX package root from a Windows bundle folder (the one that
# desktop/packaging/windows-bundle.sh assembles). Produces a directory that
# `makeappx pack /d <root>` turns into a Store-ready .msix. Does not sign:
# the Microsoft Store re-signs the package it accepts. Not tested on Windows yet.
#
# Usage, from the repository root:
#   desktop/packaging/windows-bundle.sh dist
#   desktop/packaging/windows-store/msix-layout.sh dist msix-root
#
# Result:
#   <root>/AppxManifest.xml        the manifest (copied verbatim)
#   <root>/Assets/*.png            the Store logos referenced by the manifest
#   <root>/app/...                 every file from the bundle folder, unchanged
#
# The GTK app finds its DLLs and its share\/lib\ data next to openworld-desktop.exe,
# so the whole bundle is copied into app\ with its layout intact; the manifest points
# Executable at app\openworld-desktop.exe. Nothing is remapped through a VFS folder.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
bundle=${1:?usage: msix-layout.sh BUNDLE_DIR OUT_ROOT}
root=${2:?usage: msix-layout.sh BUNDLE_DIR OUT_ROOT}

test -f "$bundle/openworld-desktop.exe" || {
  echo "no openworld-desktop.exe in $bundle; run desktop/packaging/windows-bundle.sh first" >&2
  exit 1
}

rm -rf "$root"
mkdir -p "$root/app" "$root/Assets"

cp "$here/AppxManifest.xml" "$root/AppxManifest.xml"
cp -r "$bundle"/. "$root/app/"

# The four logos the manifest names. Real icons are owner-supplied (see Assets/README.md);
# CI drops placeholder PNGs in beside this script so the package can be packed and validated.
missing=0
for png in StoreLogo.png Square150x150Logo.png Square44x44Logo.png Wide310x150Logo.png; do
  if [ -f "$here/Assets/$png" ]; then
    cp "$here/Assets/$png" "$root/Assets/$png"
  else
    echo "missing Store asset: desktop/packaging/windows-store/Assets/$png" >&2
    missing=1
  fi
done
if [ "$missing" -ne 0 ]; then
  echo "supply the PNGs listed in desktop/packaging/windows-store/Assets/README.md" >&2
  echo "(or run make-placeholder-assets.ps1 to fill them with clearly-marked placeholders)" >&2
  exit 1
fi

echo "MSIX root laid out in $root"
