#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Linux tests only. OpenSwiftUI is not a dependency of the macOS app.
set -eu
cd "$(dirname "$0")/.."
swift_bin=$(readlink -f "$(command -v swift)")
toolchain_usr=$(dirname "$(dirname "$swift_bin")")
export OPENRENDERBOX_LIB_SWIFT_PATH="$toolchain_usr/lib/swift"
export LIBRARY_PATH="${LIBRARY_PATH:+$LIBRARY_PATH:}$toolchain_usr/lib"
export LD_LIBRARY_PATH="${LD_LIBRARY_PATH:+$LD_LIBRARY_PATH:}$toolchain_usr/lib"
export OPENSWIFTUI_WERROR=0
export OPENSWIFTUI_OPENATTRIBUTESHIMS_ATTRIBUTEGRAPH=0
export OPENSWIFTUI_OPENATTRIBUTESHIMS_COMPUTE=1
export OPENSWIFTUI_OPENATTRIBUTESHIMS_COMPUTE_BINARY=0
export OPENSWIFTUI_OPENATTRIBUTESHIMS_COMPUTE_SOURCE_VERSION=0.6.0
export OPENSWIFTUI_COMPATIBILITY_TEST=0
export OPENSWIFTUI_SWIFT_LOG=1
export OPENSWIFTUI_SWIFT_CRYPTO=1
export OPENSWIFTUI_RENDER_GTK=0
if [ -z "${OPENWORLD_LIB:-}" ]; then
    for candidate in ../core/target/debug/libopenworld_core.so ../core/target/release/libopenworld_core.so; do
        if [ -f "$candidate" ]; then
            OPENWORLD_LIB=$(readlink -f "$candidate")
            export OPENWORLD_LIB
            break
        fi
    done
fi
swift test "$@"
bin="$(swift build --show-bin-path)/OpenWorld"
if [ -x "$bin" ]; then
    echo "Linux built the app. The app build belongs on a macOS runner." >&2
    exit 1
fi
swift build --product OpenWorldScreens
screen="$(swift build --show-bin-path)/OpenWorldScreens"
out="$(OPENWORLD_SCREEN=choose "$screen")"
printf '%s\n' "$out" | grep -q "Choose a photo or video" || {
    echo "The phone screen did not print." >&2
    printf '%s\n' "$out" >&2
    exit 1
}
printf '%s\n' "$out" | grep -q "OpenSwiftUI backend: stdout" || {
    echo "OpenSwiftUI stdout renderer did not run." >&2
    printf '%s\n' "$out" >&2
    exit 1
}
