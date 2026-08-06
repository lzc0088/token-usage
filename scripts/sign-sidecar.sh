#!/usr/bin/env bash
# Sign the bundled tokscale sidecar with the Developer ID identity so it
# passes Apple notarization. Runs as Tauri's beforeBundleCommand, BEFORE the
# binary is copied into the .app bundle — so the signature travels with it.
#
# Only signs when APPLE_SIGN_IDENTITY is set (CI). Local dev builds skip this
# (adhoc linker signature is fine for local use).
set -euo pipefail

BIN="${CARGO_WORKSPACE_DIR:-$(pwd)}/src-tauri/bin/tokscale"
IDENTITY="${APPLE_SIGN_IDENTITY:-}"

if [ -z "$IDENTITY" ]; then
  echo "[sign-sidecar] APPLE_SIGN_IDENTITY unset, skipping (local dev)"
  exit 0
fi

if [ ! -f "$BIN" ]; then
  echo "[sign-sidecar] $BIN not found, skipping"
  exit 0
fi

echo "[sign-sidecar] signing $BIN as \"$IDENTITY\" (hardened runtime + timestamp)"
# --force: overwrite the adhoc linker signature the binary ships with.
# --options runtime: hardened runtime (Apple requirement).
# --timestamp: secure timestamp (Apple requirement).
codesign --force --sign "$IDENTITY" --options runtime --timestamp "$BIN"
codesign --verify --strict --verbose=2 "$BIN" 2>&1 | head -3
echo "[sign-sidecar] done"
