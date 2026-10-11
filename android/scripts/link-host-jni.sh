#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Build libopenworld_core and the JNI shim the phone class loads.
# This is the host process. It is not an Android ABI and not an iOS archive.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"
profile=${OPENWORLD_PROFILE:-debug}
if [ "$profile" = release ]; then
    cargo build --release --manifest-path core/Cargo.toml -p openworld-core
    dir="$root/core/target/release"
else
    cargo build --manifest-path core/Cargo.toml -p openworld-core
    dir="$root/core/target/debug"
fi
java_bin=$(command -v java)
java_home=$(dirname "$(dirname "$(readlink -f "$java_bin")")")
gcc -shared -fPIC \
    -I"$java_home/include" -I"$java_home/include/linux" \
    "$root/android/app/src/main/cpp/openworld_jni.c" \
    -L"$dir" -lopenworld_core \
    -Wl,-rpath,'$ORIGIN' \
    -o "$dir/libopenworld_jni.so"
test -f "$dir/libopenworld_core.so"
test -f "$dir/libopenworld_jni.so"
echo "linked $dir/libopenworld_jni.so"
