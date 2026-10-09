#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Show the phone screens with OpenSwiftUI's Linux stdout renderer.
# This is not the Mac or iPhone app.
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
if [ -z "${OPENWORLD_BIN:-}" ]; then
    if [ -x ../core/target/debug/openworld ]; then
        export OPENWORLD_BIN="$(readlink -f ../core/target/debug/openworld)"
    elif [ -x ../core/target/release/openworld ]; then
        export OPENWORLD_BIN="$(readlink -f ../core/target/release/openworld)"
    fi
fi
if [ -z "${OPENWORLD_BUNDLES:-}" ] && [ -d ../bundles ]; then
    export OPENWORLD_BUNDLES="$(readlink -f ../bundles)"
fi
swift build --product OpenWorldScreens
screen="$(swift build --show-bin-path)/OpenWorldScreens"
cycle() {
    for name in choose device bundle size estimate results leaving; do
        clear
        OPENWORLD_SCREEN="$name" "$screen"
        sleep 4
    done
    sleep 3600
}
if [ "${1:-}" = "--cycle" ]; then
    cycle
    exit 0
fi
if [ -n "${DISPLAY:-}" ] && [ "${OPENWORLD_FOREGROUND:-}" != "1" ] && command -v xfce4-terminal >/dev/null 2>&1; then
    exec xfce4-terminal \
        --title="OpenWorld phone screens, OpenSwiftUI stdout" \
        --font="JetBrains Mono 14" \
        --geometry=96x48 \
        -e "env OPENWORLD_FOREGROUND=1 sh \"$0\" --cycle"
fi
cycle
