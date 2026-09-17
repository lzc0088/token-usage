#!/usr/bin/env bash
# Build the WidgetKit extension and embed it into a bundled Token Usage
# .app (PlugIns/), then (re)sign the app bundle so macOS registers the
# widget in the system widget gallery.
#
# Usage:
#   ./scripts/install-widget.sh              # release bundle
#   ./scripts/install-widget.sh --debug      # target/debug bundle (--debug build)
#   ./scripts/install-widget.sh --universal  # release + universal appex
#
# After running, launch the .app once (it exports the snapshot within ~30s)
# and the desktop widget gallery will list Token Usage.
set -euo pipefail
cd "$(dirname "$0")/.."

MODE="${2:-${1:-release}}"
case "$MODE" in
  --debug) BUNDLE_DIR="src-tauri/target/debug/bundle/macos" ;;
  *) BUNDLE_DIR="src-tauri/target/release/bundle/macos" ;;
esac

# 1. Compile + sign the .appex.
UNIVERSAL_FLAG=""
if [ "${1:-}" = "--universal" ] || [ "${2:-}" = "--universal" ]; then
  UNIVERSAL_FLAG="--universal"
fi
./macos-widget/build.sh $UNIVERSAL_FLAG

# 2. Locate the app bundle.
APP="$(ls -d "$BUNDLE_DIR"/*.app 2>/dev/null | head -1 || true)"
if [ -z "$APP" ]; then
  echo "[install-widget] no .app found under $BUNDLE_DIR — run 'npm run tauri build${MODE:+ $MODE}' first" >&2
  exit 1
fi

# 3. Embed as an app extension.
PLUGINS="$APP/Contents/PlugIns"
mkdir -p "$PLUGINS"
rm -rf "$PLUGINS/TokenUsageWidget.appex"
cp -R macos-widget/dist/TokenUsageWidget.appex "$PLUGINS/"

# 4. Re-sign the whole bundle. Identity resolution order: APPLE_SIGN_IDENTITY
#    env (CI), then the Developer ID identity from tauri.conf.json (local —
#    the keychain holds the cert). The app group entitlement MUST be present
#    on the host app too, or macOS denies Group Containers access (EPERM).
IDENTITY="${APPLE_SIGN_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
  IDENTITY="$(sed -n 's/.*"signingIdentity"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' src-tauri/tauri.conf.json | head -1)"
  # Only use it if the cert is actually in this keychain; otherwise ad-hoc.
  if [ -n "$IDENTITY" ] && ! security find-identity -v -p codesigning 2>/dev/null | grep -qF "$IDENTITY"; then
    echo "[install-widget] identity not in keychain, falling back to ad-hoc"
    IDENTITY=""
  fi
fi
if [ -n "$IDENTITY" ]; then
  if ! codesign --force --deep --options runtime \
    --entitlements src-tauri/Entitlements.plist \
    --sign "$IDENTITY" "$APP" 2>/tmp/install-widget-codesign.err; then
    echo "[install-widget] codesign failed:" >&2
    cat /tmp/install-widget-codesign.err >&2
    echo >&2
    echo "  errSecInternalComponent = codesign cannot read the private key." >&2
    echo "  Fix (once, in YOUR terminal):" >&2
    echo "    security unlock-keychain" >&2
    echo "    security set-key-partition-list -S apple-tool:,apple: -s -k '<登录密码>' ~/Library/Keychains/login.keychain-db" >&2
    echo "  then re-run this script." >&2
    exit 1
  fi
  echo "[install-widget] re-signed app with $IDENTITY (+ app-group entitlements)"
else
  codesign --force --deep --sign - "$APP"
  echo "[install-widget] re-signed app ad-hoc (widget gallery will NOT see it — app group needs a real identity)"
fi

echo "[install-widget] embedded: $PLUGINS/TokenUsageWidget.appex"
echo "[install-widget] app: $APP"
