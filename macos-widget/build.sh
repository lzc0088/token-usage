#!/usr/bin/env bash
# Build + sign the macOS WidgetKit extension (.appex) with plain swiftc —
# no Xcode project needed. Mirrors sign-sidecar.sh's identity handling:
# APPLE_SIGN_IDENTITY when set (CI/release), otherwise an ad-hoc signature
# (local dev — the widget still registers on the dev machine).
#
# Usage:
#   ./build.sh              # arm64 (dev machines)
#   ./build.sh --universal  # arm64 + x86_64 (release)
#
# Output: dist/TokenUsageWidget.appex
set -euo pipefail
cd "$(dirname "$0")"

SDK="$(xcrun --show-sdk-path)"
VERSION="$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' ../src-tauri/tauri.conf.json | head -1)"
# Identity: env (CI) → tauri.conf's Developer ID (local keychain) → ad-hoc.
# A real identity matters: the app group entitlement only grants Group
# Containers access to properly signed (non-ad-hoc) binaries.
IDENTITY="${APPLE_SIGN_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
  IDENTITY="$(sed -n 's/.*"signingIdentity"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' ../src-tauri/tauri.conf.json | head -1)"
  if [ -n "$IDENTITY" ] && ! security find-identity -v -p codesigning 2>/dev/null | grep -qF "$IDENTITY"; then
    IDENTITY=""
  fi
fi
APP_DIR="dist/TokenUsageWidget.appex/Contents/MacOS"
OUT="dist/TokenUsageWidget.appex"

rm -rf "$OUT"
mkdir -p "$APP_DIR" "dist/TokenUsageWidget.appex/Contents"

SOURCES=(Snapshot.swift Provider.swift Views.swift TokenUsageWidget.swift)

compile() {
  local target="$1" out="$2"
  swiftc \
    -swift-version 5 \
    -target "$target" \
    -sdk "$SDK" \
    -framework WidgetKit -framework SwiftUI \
    -parse-as-library -emit-executable \
    -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker Info.plist \
    -o "$out" \
    "${SOURCES[@]}"
}

# Patch the version into Info.plist (kept in-tree at 1.0.0-shape for diffs).
cp Info.plist "$OUT/Contents/Info.plist"
if [ -n "${VERSION:-}" ]; then
  plutil -replace CFBundleShortVersionString -string "$VERSION" "$OUT/Contents/Info.plist"
fi

if [ "${1:-}" = "--universal" ]; then
  compile arm64-apple-macos14.0 dist/w-arm64
  compile x86_64-apple-macos14.0 dist/w-x86_64
  lipo -create -output "$APP_DIR/TokenUsageWidget" dist/w-arm64 dist/w-x86_64
  rm -f dist/w-arm64 dist/w-x86_64
else
  compile arm64-apple-macos14.0 "$APP_DIR/TokenUsageWidget"
fi

if [ -n "$IDENTITY" ]; then
  codesign --force --identifier com.tokenusage.desktop.widget \
    --options runtime \
    --entitlements TokenUsageWidget.entitlements \
    --sign "$IDENTITY" "$OUT"
  echo "[build-widget] signed with identity: $IDENTITY"
else
  codesign --force --identifier com.tokenusage.desktop.widget \
    --entitlements TokenUsageWidget.entitlements \
    --sign - "$OUT"
  echo "[build-widget] ad-hoc signed (set APPLE_SIGN_IDENTITY for release signing)"
fi

echo "[build-widget] built: $OUT"
