#!/usr/bin/env bash
# Builds crates/core-uniffi for iOS and Android and generates the Swift/Kotlin
# bindings consumed by ios/PhotosCoreModule.swift and PhotosCoreModule.kt.
#
# Prerequisites:
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios \
#     aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
#   cargo install cargo-ndk
#   Xcode (for xcodebuild -create-xcframework), Android NDK (ANDROID_NDK_HOME)
#
# Usage: build-rust.sh [ios|android|all] [debug|release]
set -euo pipefail

PLATFORM="${1:-all}"
PROFILE="${2:-release}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$PKG/../.." && pwd)"
CRATE="photos-core-uniffi"
LIB="libphotos_core_ffi"
CARGO_FLAGS=()
[[ "$PROFILE" == "release" ]] && CARGO_FLAGS+=(--release)

cd "$ROOT"

bindgen() {
  local lang="$1" out="$2" lib="$3"
  cargo run -p "$CRATE" --features bindgen --bin uniffi-bindgen -- \
    generate --library "$lib" --language "$lang" --out-dir "$out"
}

build_ios() {
  local targets=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
  for t in "${targets[@]}"; do
    cargo build -p "$CRATE" --target "$t" "${CARGO_FLAGS[@]}"
  done

  local gen="$PKG/ios/generated"
  rm -rf "$gen" && mkdir -p "$gen"
  bindgen swift "$gen" "target/aarch64-apple-ios/$PROFILE/$LIB.a"

  # UniFFI emits a modulemap named <name>FFI.modulemap; CocoaPods needs module.modulemap.
  mv "$gen"/*.modulemap "$gen/module.modulemap"

  local sim="target/ios-sim-universal"
  mkdir -p "$sim"
  lipo -create \
    "target/aarch64-apple-ios-sim/$PROFILE/$LIB.a" \
    "target/x86_64-apple-ios/$PROFILE/$LIB.a" \
    -output "$sim/$LIB.a"

  xcodebuild -create-xcframework \
    -library "target/aarch64-apple-ios/$PROFILE/$LIB.a" -headers "$gen" \
    -library "$sim/$LIB.a" -headers "$gen" \
    -output "$gen/PhotosCoreFFI.xcframework"
}

build_android() {
  local jni="$PKG/android/src/main/jniLibs"
  rm -rf "$jni"
  cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o "$jni" \
    build -p "$CRATE" "${CARGO_FLAGS[@]}"

  local gen="$PKG/android/src/main/java"
  rm -rf "$gen/uniffi"
  bindgen kotlin "$gen" "target/aarch64-linux-android/$PROFILE/$LIB.so"
}

case "$PLATFORM" in
  ios) build_ios ;;
  android) build_android ;;
  all) build_ios; build_android ;;
  *) echo "unknown platform: $PLATFORM" >&2; exit 2 ;;
esac
