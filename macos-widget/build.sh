#!/usr/bin/env bash
# Build the macOS WidgetKit extension + helpers with plain swiftc — no Xcode
# project. Everything is AD-HOC signed (bundle integrity, not distribution):
# this works for local builds and unreleased distribution alike; the system
# ignores App Groups without a Team ID, so sharing runs through the publisher
# helper's UserDefaults (see Publisher.swift).
#
# Usage:
#   ./build.sh              # current arch
#   ./build.sh --universal  # arm64 + x86_64
#
# Output:
#   dist/TokenUsageWidget.appex
#   dist/Helpers/token-usage-widget-publish
#   dist/Helpers/token-usage-widget-reload
set -euo pipefail
cd "$(dirname "$0")"

# Non-macOS (CI matrix): nothing to do.
if [ "$(uname -s)" != "Darwin" ]; then
  echo "[build-widget] non-macOS ($(uname -s)), skipping"
  exit 0
fi

SDK="$(xcrun --show-sdk-path)"
# Version MUST match the host app's — WidgetKit archives the extension's
# bundle stub into every timeline and validates it against LaunchServices;
# a mismatched build number leaves upgraded installs with incompatible
# cached stubs (Metrik hit exactly this).
VERSION="$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' ../package.json | head -1)"

if [ "${1:-}" = "--universal" ]; then
  ARCHS=(arm64 x86_64)
elif [ "${TAURI_ENV_ARCH:-}" = "universal" ]; then
  ARCHS=(arm64 x86_64)
else
  ARCHS=("$(uname -m)")
fi

APPEX="dist/TokenUsageWidget.appex"
rm -rf dist
mkdir -p "$APPEX/Contents/MacOS" "dist/Helpers" "dist/arch"

build_universal() {
  local out="$1"; shift
  local name; name="$(basename "$out")"
  local bins=()
  for arch in "${ARCHS[@]}"; do
    local arch_out="dist/arch/$name-$arch"
    swiftc -parse-as-library -O \
      -target "${arch}-apple-macos14.0" \
      -sdk "$SDK" \
      "$@" \
      -o "$arch_out"
    bins+=("$arch_out")
  done
  if [ "${#bins[@]}" -eq 1 ]; then
    cp "${bins[0]}" "$out"
  else
    lipo -create "${bins[@]}" -output "$out"
  fi
  chmod +x "$out"
}

echo "[build-widget] building for ${ARCHS[*]}..."

# 1. The widget extension itself. -application-extension restricts to
#    extension-safe APIs and the explicit _NSExtensionMain entry point is
#    what WidgetKit requires.
build_universal "$APPEX/Contents/MacOS/TokenUsageWidget" \
  -application-extension \
  -framework AppKit -framework SwiftUI -framework WidgetKit \
  -Xlinker -e -Xlinker _NSExtensionMain \
  Snapshot.swift Provider.swift Views.swift TokenUsageWidget.swift

cp Info.plist "$APPEX/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APPEX/Contents/Info.plist" > /dev/null
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APPEX/Contents/Info.plist" > /dev/null

# 2. Publisher: host spawns it with the snapshot JSON on stdin; it embeds
#    the WIDGET's bundle id so UserDefaults lands in the widget container.
#    The embedded Info.plist is mandatory — a sandboxed bare executable
#    SIGTRAPs in libsecinit without a bundle identifier.
build_universal "dist/Helpers/token-usage-widget-publish" \
  -framework Foundation \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker Publisher.Info.plist \
  Publisher.swift

# 3. Reloader: host spawns it after publishing; embeds the HOST app's
#    bundle id so WidgetCenter attributes the reload to Token Usage.
build_universal "dist/Helpers/token-usage-widget-reload" \
  -framework WidgetKit \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker Reloader.Info.plist \
  Reloader.swift

rm -rf dist/arch

# 4. Ad-hoc bundle integrity signing (no Developer ID needed).
codesign --force --sign - \
  --entitlements TokenUsageWidget.entitlements \
  "$APPEX"
codesign --force --sign - \
  --entitlements Publisher.entitlements \
  dist/Helpers/token-usage-widget-publish
codesign --force --sign - dist/Helpers/token-usage-widget-reload
codesign --verify --strict "$APPEX"

echo "[build-widget] built: $APPEX + 2 helpers (ad-hoc signed)"
