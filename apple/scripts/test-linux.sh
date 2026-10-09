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
bin="$(swift build --show-bin-path)/OpenWorld"
if [ -x "$bin" ]; then
    echo "Linux built the app. The app build belongs on a macOS runner." >&2
    exit 1
fi
exec swift test "$@"
