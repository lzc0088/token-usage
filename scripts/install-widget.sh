#!/usr/bin/env bash
# DEV shortcut: build the widget extension + helpers and hand them to a
# `tauri build --debug`-produced app bundle. In release/CI builds the
# tauri.conf.json resources mapping embeds these automatically
# (beforeBundleCommand runs build.sh) — this script only exists for local
# iteration on an existing bundle without a full rebuild.
set -euo pipefail
cd "$(dirname "$0")/.."

./macos-widget/build.sh

BUNDLE_DIR="src-tauri/target/debug/bundle/macos"
APP="$(ls -d "$BUNDLE_DIR"/*.app 2>/dev/null | head -1 || true)"
if [ -z "$APP" ]; then
  BUNDLE_DIR="src-tauri/target/release/bundle/macos"
  APP="$(ls -d "$BUNDLE_DIR"/*.app 2>/dev/null | head -1 || true)"
fi
if [ -z "$APP" ]; then
  echo "[install-widget] no .app bundle found — run 'npm run tauri build' first" >&2
  exit 1
fi

mkdir -p "$APP/Contents/PlugIns" "$APP/Contents/Helpers"
rm -rf "$APP/Contents/PlugIns/TokenUsageWidget.appex"
cp -R macos-widget/dist/TokenUsageWidget.appex "$APP/Contents/PlugIns/"
cp macos-widget/dist/Helpers/* "$APP/Contents/Helpers/"
# Ad-hoc re-seal the bundle (extension must be sealed inside the host).
codesign --force --deep --sign - "$APP"

echo "[install-widget] embedded into: $APP"
echo "[install-widget] launch it once, then check the desktop widget gallery."
