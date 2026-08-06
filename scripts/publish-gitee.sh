#!/usr/bin/env bash
#
# Upload release installers (built by GitHub Actions) to a Gitee Release.
#
# Run by the `publish-gitee` job in .github/workflows/release.yml after the
# three-platform `bundle` job uploads its artifacts. Locally testable with
# DRY_RUN=1 (prints the curl calls instead of executing them).
#
# Env:
#   GITEE_ACCESS_TOKEN  (required) Gitee personal access token (projects scope).
#   TAG                 (required) release tag, e.g. v1.0.1 (usually github.ref_name).
#   OWNER               (default lzc0088) Gitee repo owner.
#   REPO                (default token-usage) Gitee repo name.
#   ARTIFACT_DIR        (default artifacts) where downloaded workflow artifacts live.
#   DRY_RUN             (default unset) any non-empty value → print, don't transmit.
#   TARGET_COMMITISH    (default master) commit/branch the release tag points at.
#
# Exits non-zero if any installer fails to upload after retries, so the CI job
# fails loudly and can be re-run (re-running is safe: an existing release is
# reused and already-uploaded files are skipped).

set -euo pipefail

OWNER="${OWNER:-lzc0088}"
REPO="${REPO:-token-usage}"
TAG="${TAG:?TAG is required (e.g. v1.0.1)}"
TOKEN="${GITEE_ACCESS_TOKEN:?GITEE_ACCESS_TOKEN is required}"
ARTIFACT_DIR="${ARTIFACT_DIR:-artifacts}"
TARGET_COMMITISH="${TARGET_COMMITISH:-master}"
DRY_RUN="${DRY_RUN:-}"
MAX_RETRIES=3
API="https://gitee.com/api/v5/repos/${OWNER}/${REPO}"

# ── helpers ────────────────────────────────────────────────────────────────

log() { printf '\033[1;34m[gitee]\033[0m %s\n' "$*" >&2; }
warn() { printf '\033[1;33m[gitee]\033[0m %s\n' "$*" >&2; }

# curl for fast JSON API calls (create/get release, list assets). Silent,
# short timeouts, auto-retry on transient errors. Respects DRY_RUN.
# Args: a descriptive label, then curl args.
curl_api() {
  local label="$1"; shift
  if [ -n "$DRY_RUN" ]; then
    log "DRY-RUN $label — would call:"
    printf '      curl %s\n' "$*"
    return 0
  fi
  curl --fail --silent --show-error \
    --connect-timeout 30 --max-time 60 \
    --retry "$MAX_RETRIES" --retry-delay 2 "$@"
}

# curl for file uploads (slow, cross-border to Gitee). Shows a progress bar so
# a slow upload isn't mistaken for a hang, with a generous transfer cap so a
# stalled TCP connection eventually fails instead of hanging forever.
# Args: a descriptive label, then curl args.
curl_upload() {
  local label="$1"; shift
  if [ -n "$DRY_RUN" ]; then
    log "DRY-RUN $label — would call:"
    printf '      curl %s\n' "$*"
    return 0
  fi
  # --progress-bar writes transfer speed to stderr (visible in Actions logs).
  curl --fail --show-error --progress-bar \
    --connect-timeout 30 --max-time 1800 \
    --retry "$MAX_RETRIES" --retry-delay 2 --retry-all-errors "$@"
}

# ── 1. collect installers ──────────────────────────────────────────────────

FILES=()
while IFS= read -r line; do
  [ -n "$line" ] && FILES+=("$line")
done < <(
  find "$ARTIFACT_DIR" -type f \( \
    -name '*.dmg' -o -name '*.pkg' \
    -o -name '*.msi' -o -name '*.exe' \
    -o -name '*.deb' -o -name '*.AppImage' -o -name '*.rpm' \
  \) 2>/dev/null | sort
)

if [ "${#FILES[@]}" -eq 0 ]; then
  warn "no installers found under $ARTIFACT_DIR/ (expected .dmg/.msi/.exe/.deb/.AppImage)"
  exit 1
fi

log "found ${#FILES[@]} installer(s):"
for f in "${FILES[@]}"; do log "  - $(basename "$f")"; done

# ── 2. resolve release id (create or fetch existing) ───────────────────────

resolve_release_id() {
  # DRY_RUN: don't hit the API; return a fake id so the upload loop can still
  # print the curl calls it would make.
  if [ -n "$DRY_RUN" ]; then
    log "DRY-RUN: skipping release create/get, using fake id"
    echo "000000"
    return 0
  fi
  # Try to create first. A repeat run for the same tag returns a conflict →
  # fall back to GET by tag.
  local create_resp payload
  payload=$(jq -nc --arg token "$TOKEN" --arg t "$TAG" --arg c "$TARGET_COMMITISH" \
    '{access_token:$token, tag_name:$t, name:$t, body:("Token Usage "+$t), target_commitish:$c}')
  if create_resp=$(curl_api "create release" \
        -X POST "$API/releases" \
        -H "Content-Type: application/json" \
        -d "$payload" 2>&1); then
    local id
    id=$(echo "$create_resp" | jq -r '.id // empty' 2>/dev/null || true)
    if [ -n "$id" ]; then
      log "created release id=$id"
      echo "$id"
      return 0
    fi
  fi
  warn "create failed or no id in response, trying GET by tag"
  local existing
  existing=$(curl_api "get release by tag" \
              "$API/releases/tags/$TAG?access_token=$TOKEN")
  local id
  id=$(echo "$existing" | jq -r '.id // empty' 2>/dev/null || true)
  if [ -z "$id" ]; then
    warn "could not resolve release id for tag $TAG; create response was:"
    printf '%s\n' "$create_resp" >&2
    return 1
  fi
  log "reusing existing release id=$id"
  echo "$id"
}

RELEASE_ID=$(resolve_release_id) || { warn "aborting: no release id"; exit 1; }
log "release id: $RELEASE_ID"
log "release page: https://gitee.com/${OWNER}/${REPO}/releases/${TAG}"

# ── 3. list already-uploaded assets (so we can skip on re-run) ─────────────

ALREADY_UPLOADED="$(
  curl_api "list assets" "$API/releases/$RELEASE_ID?access_token=$TOKEN" \
    | jq -r '.assets[].name // empty' 2>/dev/null || true
)"

# ── 4. upload each installer ───────────────────────────────────────────────

uploaded=0
skipped=0
failed=0

for f in "${FILES[@]}"; do
  name="$(basename "$f")"

  # Skip if an asset with the same name already exists (idempotent re-runs).
  if grep -qxF "$name" <<<"$ALREADY_UPLOADED" 2>/dev/null; then
    log "skip    $name (already uploaded)"
    skipped=$((skipped + 1))
    continue
  fi

  log "upload  $name ($(du -h "$f" | cut -f1))"
  resp=""
  ok=0
  for attempt in $(seq 1 "$MAX_RETRIES"); do
    # NOTE: -F builds multipart/form-data; Gitee expects field `file`.
    # Upload is the slow step (cross-border GitHub runner → Gitee); progress
    # bar + max-time keep it observable and bounded.
    if resp=$(curl_upload "upload $name (try $attempt)" \
          -X POST "$API/releases/$RELEASE_ID/attach_files" \
          -F "access_token=$TOKEN" \
          -F "file=@$f" 2>&1); then
      ok=1
      break
    fi
    warn "  attempt $attempt failed; retrying in ${attempt}s…"
    [ -n "$DRY_RUN" ] || sleep "$attempt"
  done

  if [ "$ok" -ne 1 ]; then
    warn "FAILED   $name after $MAX_RETRIES attempts"
    printf '%s\n' "$resp" >&2
    failed=$((failed + 1))
    continue
  fi

  # Sanity-check the returned asset carries a download URL that update.rs can
  # resolve (it reads `browser_download_url`, fallback `download_url`).
  dl_url=$(echo "$resp" | jq -r '.browser_download_url // .download_url // empty' 2>/dev/null || true)
  if [ -z "$dl_url" ] && [ -z "$DRY_RUN" ]; then
    warn "  uploaded, but response has no browser_download_url/download_url — update.rs may not resolve it:"
    printf '%s\n' "$resp" >&2
  fi
  log "  → $dl_url"
  uploaded=$((uploaded + 1))
done

# ── 5. summary ─────────────────────────────────────────────────────────────

log "done: uploaded=$uploaded skipped=$skipped failed=$failed"
if [ "$failed" -ne 0 ]; then
  warn "$failed file(s) failed; re-run the job to retry (already-uploaded files are skipped)"
  exit 1
fi

log "release: https://gitee.com/${OWNER}/${REPO}/releases/${TAG}"
