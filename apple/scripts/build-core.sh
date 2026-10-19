#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Builds the Rust core for Apple platforms and packages it for Xcode (macOS only):
#
#   apple/build/OpenWorldCore.xcframework  libopenworld_core.a for iOS, the iOS simulator, and
#                                          macOS, with include/openworld.h and a module map
#                                          (module OpenWorldCore) so Swift calls ow_call directly.
#   apple/build/onnxruntime.xcframework    ONNX Runtime's official static C/C++ xcframework.
#
# ONNX Runtime comes from the onnxruntime-c CocoaPods archive that Microsoft's
# onnxruntime-swift-package-manager 1.24.2 pins as its binary target:
#
#   https://download.onnxruntime.ai/pod-archive-onnxruntime-c-1.24.2.zip
#   SHA-256 f7100a992d2a8135168c8afd831e6a58b465349101982aa58b3e11d36e600b54
#
# The digest is the `checksum:` published in that package's Package.swift at tag 1.24.2
# (github.com/microsoft/onnxruntime-swift-package-manager). The archive is kept only when its
# SHA-256 matches. ONNX Runtime 1.24 serves C API version 24, which the core's `ort` crate asks
# for (feature api-24). To move to a newer release, take the URL and checksum from that
# package's Package.swift at the new tag and set ORT_POD_URL and ORT_POD_SHA256.
#
# The core is built with `--no-default-features --features fixture,coreml`: no ort-dynamic, so
# ONNX Runtime is linked statically, and Core ML is tried before the CPU. Only the staticlib is
# built (cargo rustc --crate-type staticlib), so nothing is linked here. Xcode links
# libopenworld_core.a, onnxruntime.xcframework, libc++ and CoreML into OpenWorldKit.
#   iOS and the simulator: ORT_IOS_XCFWK_PATH points ort-sys at the xcframework.
#   macOS: ort-sys has no xcframework path for macOS. ORT_LIB_LOCATION with
#   ORT_PREFER_DYNAMIC_LINK=1 makes it record a plain -lonnxruntime instead of bundling a copy
#   into the archive; the framework link in Xcode provides the symbols.
#
# Rust targets: aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin (added here).
# OPENWORLD_X86=1 adds x86_64-apple-darwin and x86_64-apple-ios. Then remove EXCLUDED_ARCHS
# from project.yml.
set -euo pipefail

apple="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$apple/.." && pwd)"
build="$apple/build"
ort_url="${ORT_POD_URL:-https://download.onnxruntime.ai/pod-archive-onnxruntime-c-1.24.2.zip}"
ort_sha="${ORT_POD_SHA256:-f7100a992d2a8135168c8afd831e6a58b465349101982aa58b3e11d36e600b54}"
export IPHONEOS_DEPLOYMENT_TARGET=17.0
export MACOSX_DEPLOYMENT_TARGET=14.0

mkdir -p "$build/cache"

# ONNX Runtime, pinned by SHA-256.
archive="$build/cache/$(basename "$ort_url")"
if [ ! -f "$archive" ] || [ "$(shasum -a 256 "$archive" | cut -d' ' -f1)" != "$ort_sha" ]; then
  curl -fL --retry 3 -o "$archive.part" "$ort_url"
  got="$(shasum -a 256 "$archive.part" | cut -d' ' -f1)"
  if [ "$got" != "$ort_sha" ]; then
    rm -f "$archive.part"
    echo "ONNX Runtime archive SHA-256 $got is not the pin $ort_sha. Refusing." >&2
    exit 1
  fi
  mv "$archive.part" "$archive"
fi
rm -rf "$build/ort-unpacked" "$build/onnxruntime.xcframework"
mkdir -p "$build/ort-unpacked"
unzip -q "$archive" -d "$build/ort-unpacked"
found="$(find "$build/ort-unpacked" -maxdepth 3 -type d -name onnxruntime.xcframework | head -n 1)"
if [ -z "$found" ]; then
  echo "onnxruntime.xcframework is not in $(basename "$archive"). Refusing." >&2
  exit 1
fi
mv "$found" "$build/onnxruntime.xcframework"
rm -rf "$build/ort-unpacked"
ort="$build/onnxruntime.xcframework"
slice() {
  find "$ort" -maxdepth 1 -type d -name "$1" | head -n 1
}
ios_slice="$(slice 'ios-arm64')"
sim_slice="$(slice 'ios-arm64*-simulator')"
mac_slice="$(slice 'macos-arm64*')"
for s in "$ios_slice" "$sim_slice" "$mac_slice"; do
  if [ -z "$s" ]; then
    echo "onnxruntime.xcframework lacks an iOS, simulator, or macOS slice:" >&2
    ls "$ort" >&2
    exit 1
  fi
done

# The Rust core, one static library per target.
targets=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin)
if [ "${OPENWORLD_X86:-0}" = "1" ]; then
  targets+=(x86_64-apple-ios x86_64-apple-darwin)
fi
if command -v rustup >/dev/null; then
  (cd "$repo" && rustup target add "${targets[@]}")
fi

build_core() {
  local target="$1"
  shift
  echo "== openworld-core for $target"
  (cd "$repo" && env "$@" cargo rustc \
    --manifest-path core/Cargo.toml -p openworld-core --lib --release \
    --target "$target" --no-default-features --features fixture,coreml \
    --crate-type staticlib -- --print native-static-libs) 2> >(tee "$build/cargo-$target.log" >&2)
}

for target in "${targets[@]}"; do
  case "$target" in
    *-apple-ios | *-apple-ios-sim)
      if [ "$target" = "x86_64-apple-ios" ]; then
        build_core "$target" ORT_LIB_LOCATION="$sim_slice" ORT_PREFER_DYNAMIC_LINK=1
      else
        build_core "$target" ORT_IOS_XCFWK_PATH="$ort"
      fi
      ;;
    *-apple-darwin)
      build_core "$target" ORT_LIB_LOCATION="$mac_slice" ORT_PREFER_DYNAMIC_LINK=1
      ;;
  esac
done

lib() {
  echo "$repo/core/target/$1/release/libopenworld_core.a"
}
ios_lib="$(lib aarch64-apple-ios)"
sim_lib="$(lib aarch64-apple-ios-sim)"
mac_lib="$(lib aarch64-apple-darwin)"
if [ "${OPENWORLD_X86:-0}" = "1" ]; then
  mkdir -p "$build/fat"
  lipo -create "$sim_lib" "$(lib x86_64-apple-ios)" -output "$build/fat/libopenworld_core-sim.a"
  lipo -create "$mac_lib" "$(lib x86_64-apple-darwin)" -output "$build/fat/libopenworld_core-mac.a"
  sim_lib="$build/fat/libopenworld_core-sim.a"
  mac_lib="$build/fat/libopenworld_core-mac.a"
fi

# The C header and the module map, the same for every slice.
headers="$build/headers"
rm -rf "$headers" "$build/OpenWorldCore.xcframework"
mkdir -p "$headers"
cp "$repo/include/openworld.h" "$headers/"
cp "$apple/Core/module.modulemap" "$headers/"

xcodebuild -create-xcframework \
  -library "$ios_lib" -headers "$headers" \
  -library "$sim_lib" -headers "$headers" \
  -library "$mac_lib" -headers "$headers" \
  -output "$build/OpenWorldCore.xcframework"

echo "Built $build/OpenWorldCore.xcframework and $build/onnxruntime.xcframework"
grep -h "native-static-libs" "$build"/cargo-*.log | sort -u || true
