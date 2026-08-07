//! Update check: query GitHub Releases API for the latest version.
//!
//! Uses `ureq` (blocking) inside an async Tauri command — `ureq` is sync but
//! Tauri runs commands on a blocking thread pool, so this is fine.

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

/// Response from the update check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// Whether a newer version is available.
    pub has_update: bool,
    /// Latest version tag (e.g. "v0.2.0").
    pub version: String,
    /// Release name / title.
    pub name: String,
    /// Markdown changelog body (may be empty for non-markdown releases).
    pub changelog: String,
    /// URL to the release page (for download).
    pub url: String,
    /// When the release was published (ISO 8601), if available.
    pub published_at: Option<String>,
    /// Error message when the check failed (network, API error, etc.).
    /// Empty when the check succeeded.
    pub error: String,
    /// Machine-readable failure kind: "" (ok) | "rate_limited" | "network"
    /// | "api_error" | "parse". Lets the UI localize + decide whether to retry.
    #[serde(default)]
    pub error_kind: String,
    /// Direct download URL for the first release asset (e.g. .dmg file).
    /// None when no asset is available.
    pub download_url: Option<String>,
}

/// Progress events pushed to the frontend during `install_update`.
/// Drives the install UI state machine (idle → downloading → installing → relaunching).
#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    /// Download started; `content_length` is total bytes (0 if unknown).
    Started { content_length: u64 },
    /// A chunk was downloaded; frontend accumulates `chunk_length`.
    Progress { chunk_length: u64 },
    /// Download finished, about to install.
    Finished,
    /// Installation complete, app is about to restart.
    Installed,
    /// Any error during check / download / install.
    Error { message: String },
}

// ── persistent cooldown (app_config KV) ─────────────────────────────────────
//
// update checks are rate-limited by GitHub (~60/hour unauthenticated).
// We persist the last check timestamp + last-known-good result so:
//   - within the cooldown we short-circuit to the cached result (no network);
//   - on a transient failure we surface the last-known result instead of
//     misleading the UI with `has_update: false` (the prior bug).

/// Cooldown for the background (auto) update check: within this window since
/// the last check we short-circuit to the cached result. 1h balances freshness
/// (a newly published release shows up within the hour) against GitHub's
/// ~60 req/hour unauthenticated rate limit. Manual checks via the "Check
/// Update" button pass `force=true` to bypass this entirely.
const COOLDOWN_MS: i64 = 60 * 60 * 1000;
const KV_LAST_CHECK_MS: &str = "update_last_check_ms";
const KV_LAST_KNOWN: &str = "update_last_known";

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Decide whether a new network check can be skipped because the cached result
/// is still within the cooldown. Pure (no I/O) so it's unit-testable.
fn within_cooldown(last_check_ms: i64, now_ms: i64) -> bool {
    now_ms - last_check_ms < COOLDOWN_MS
}

/// Load cached (last-check-ms, last-known UpdateInfo) from the app_config KV.
fn load_cached(conn: &rusqlite::Connection) -> (Option<i64>, Option<UpdateInfo>) {
    let last_check = crate::config::get_json::<i64>(conn, KV_LAST_CHECK_MS)
        .ok()
        .flatten();
    let last_known = crate::config::get_json::<UpdateInfo>(conn, KV_LAST_KNOWN)
        .ok()
        .flatten();
    (last_check, last_known)
}

/// Persist the attempt timestamp (always) and the successful result (on Ok).
fn persist(conn: &rusqlite::Connection, last_check_ms: i64, success: Option<&UpdateInfo>) {
    let _ = crate::config::set_json(conn, KV_LAST_CHECK_MS, &last_check_ms);
    if let Some(info) = success {
        let _ = crate::config::set_json(conn, KV_LAST_KNOWN, info);
    }
}

/// Parse a repo string into (owner, repo_name). Accepts:
///   "owner/repo", "github.com/owner/repo", "<host>/owner/repo".
/// Always resolves to owner + repo_name for the GitHub Releases API.
fn parse_repo(raw: &str) -> (String, String) {
    let parts: Vec<&str> = raw.split('/').filter(|s| !s.is_empty()).collect();
    // Strip the host if present (github.com/... → owner/repo)
    let (owner, repo_name): (&str, String) = if parts.len() >= 3 && parts[0].contains('.') {
        let o = parts
            .get(parts.len().saturating_sub(2))
            .copied()
            .unwrap_or("");
        let r = parts.last().copied().unwrap_or("");
        (o, r.to_string())
    } else {
        let o = parts.first().copied().unwrap_or("");
        let r = parts.get(1..).unwrap_or(&[]).join("/");
        (o, r)
    };
    (owner.to_string(), repo_name)
}

/// Build the GitHub Releases API URL for the latest release.
fn api_url(owner: &str, repo: &str) -> String {
    format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        owner, repo
    )
}

/// Build the GitHub release web page URL for a given tag.
fn release_url(owner: &str, repo: &str, tag: &str) -> String {
    format!("https://github.com/{}/{}/releases/tag/{}", owner, repo, tag)
}

/// Query the releases API and compare with the current version.
///
/// Cooldown: within `COOLDOWN_MS` (1h) of the last check, short-circuit to
/// the cached result without hitting the network — GitHub rate-limits
/// unauthenticated requests to ~60/hour. Pass `force=Some(true)` to bypass
/// (the manual "Check Update" button does this).
///
/// On a transient failure (network / rate-limit / API error), the last-known
/// result is returned if present (so the UI never forgets an available update
/// due to a flaky network); only when there is no cache do we surface the error
/// with a machine-readable `error_kind`.
#[tauri::command]
pub fn check_update(
    state: State<AppState>,
    repo: String,
    current_version: String,
    force: Option<bool>,
) -> Result<UpdateInfo, String> {
    let (owner, repo_name) = parse_repo(&repo);
    let force = force.unwrap_or(false);

    // ── 1. Cooldown short-circuit (cached path, no network) ────────────────
    // Skipped when force=true — manual "Check Update" button always hits the
    // network so the user sees the freshest state.
    if !force {
        let conn = state.db_read();
        let (last_check, last_known) = load_cached(&conn);
        if let (Some(last), Some(known)) = (last_check, last_known) {
            let now = now_ms();
            if within_cooldown(last, now) {
                tracing::debug!("update check skipped: within {}ms cooldown", COOLDOWN_MS);
                return Ok(known);
            }
        }
    } // conn guard dropped before the network call

    // ── 2. Network call ─────────────────────────────────────────────────────
    let url = api_url(&owner, &repo_name);

    let resp_result = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .call();

    // On any failure: record the attempt timestamp, then return last-known if
    // we have it (don't mislead the UI with has_update:false), else the error.
    let handle_failure = |kind: &str, msg: String, last_known: Option<UpdateInfo>| -> UpdateInfo {
        {
            let conn = state.db_write();
            persist(&conn, now_ms(), None);
        }
        if let Some(known) = last_known {
            return known;
        }
        UpdateInfo {
            has_update: false,
            version: String::new(),
            name: String::new(),
            changelog: String::new(),
            url: String::new(),
            published_at: None,
            error: msg,
            error_kind: kind.to_string(),
            download_url: None,
        }
    };

    let last_known_for_failure = {
        let conn = state.db_read();
        load_cached(&conn).1
    };

    let resp = match resp_result {
        Ok(r) => r,
        Err(ureq::Error::Status(429, _)) => {
            return Ok(handle_failure(
                "rate_limited",
                "请求过于频繁，已触发限流，请稍后再试".into(),
                last_known_for_failure,
            ));
        }
        Err(ureq::Error::Status(403, r)) => {
            // GitHub returns 403 for rate-limit exhaustion; sniff the body.
            let body = r.into_string().unwrap_or_default();
            let lower = body.to_lowercase();
            let kind = if lower.contains("rate") || lower.contains("limit") {
                "rate_limited"
            } else {
                "api_error"
            };
            return Ok(handle_failure(
                kind,
                format!(
                    "API 返回 403：{}",
                    body.chars().take(200).collect::<String>()
                ),
                last_known_for_failure,
            ));
        }
        Err(ureq::Error::Status(code, _)) => {
            return Ok(handle_failure(
                "api_error",
                format!("API 返回错误 {}", code),
                last_known_for_failure,
            ));
        }
        Err(ureq::Error::Transport(e)) => {
            return Ok(handle_failure(
                "network",
                format!("网络请求失败：{}", e),
                last_known_for_failure,
            ));
        }
    };

    if resp.status() != 200 {
        return Ok(handle_failure(
            "api_error",
            format!(
                "API 返回错误 {} ({}): 请确认仓库地址正确且已开启 Release 功能",
                resp.status(),
                url
            ),
            last_known_for_failure,
        ));
    }

    let body: serde_json::Value = {
        let reader = resp.into_reader();
        match serde_json::from_reader(reader) {
            Ok(v) => v,
            Err(e) => {
                return Ok(handle_failure(
                    "parse",
                    format!("解析响应失败：{}", e),
                    last_known_for_failure,
                ));
            }
        }
    };

    let latest_tag = body
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let latest_name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&latest_tag)
        .to_string();

    let changelog = body
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let html_url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .map(|u| u.to_string())
        .unwrap_or_else(|| release_url(&owner, &repo_name, &latest_tag));

    let published_at = body
        .get("published_at")
        .or_else(|| body.get("created_at"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let has_update = is_newer(&current_version, &latest_tag);

    // Pick the asset matching the current platform/arch from the release's
    // assets list (e.g. macOS aarch64 → pick the aarch64 .dmg). Falls back to
    // None when there are no matching assets.
    let download_url = pick_matching_asset(&body);

    let info = UpdateInfo {
        has_update,
        version: latest_tag,
        name: latest_name,
        changelog,
        url: html_url,
        published_at,
        error: String::new(),
        error_kind: String::new(),
        download_url,
    };

    // ── 3. Persist the successful result ─────────────────────────────────────
    {
        let conn = state.db_write();
        persist(&conn, now_ms(), Some(&info));
    }

    Ok(info)
}

/// Pick the release asset matching the current OS/architecture.
/// Returns its browser_download_url, or None when no asset matches.
/// Falls back to the first asset's URL when the platform is unknown.
fn pick_matching_asset(body: &serde_json::Value) -> Option<String> {
    let assets = body.get("assets")?.as_array()?;
    if assets.is_empty() {
        return None;
    }

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    // Valid extensions + the preferred one per platform.
    let (valid_exts, prefer_ext): (&[&str], &str) = match os {
        "macos" => (&[".dmg", ".pkg", ".app"], ".dmg"),
        "windows" => (&[".msi", ".exe"], ".msi"),
        "linux" => (&[".appimage", ".deb", ".rpm"], ".appimage"),
        _ => return asset_url(assets.first()?),
    };

    // Architecture keywords. A filename containing a "wrong" arch keyword is
    // disqualified so x86_64 builds aren't offered on Apple Silicon (and vice
    // versa). Empty arrays mean "no arch keyword to check".
    let (right_kw, wrong_kw): (&[&str], &[&str]) = match (os, arch) {
        ("macos", "aarch64") => (
            &["aarch64", "arm64", "apple-silicon", "apple"],
            &["x64", "x86_64", "x86", "intel"],
        ),
        ("macos", "x86_64") => (
            &["x64", "x86_64", "intel"],
            &["aarch64", "arm64", "apple-silicon"],
        ),
        ("windows", "x86_64") => (&["x64", "x86_64"], &["aarch64", "arm64"]),
        ("windows", "aarch64") => (&["aarch64", "arm64"], &["x64", "x86_64"]),
        ("linux", "x86_64") => (&["x64", "x86_64", "amd64"], &["aarch64", "arm64"]),
        ("linux", "aarch64") => (&["aarch64", "arm64"], &["x64", "x86_64", "amd64"]),
        _ => (&[], &[]),
    };

    let mut best: Option<(i32, String)> = None;
    for asset in assets {
        let name = asset.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let url = match asset_url(asset) {
            Some(u) => u,
            None => continue,
        };
        let lower = name.to_lowercase();

        // Must be a valid extension for this platform.
        if !valid_exts
            .iter()
            .any(|ext| lower.ends_with(&ext.to_lowercase()))
        {
            continue;
        }
        // Disqualify assets tagged with the wrong architecture.
        if !wrong_kw.is_empty() && wrong_kw.iter().any(|kw| lower.contains(kw)) {
            continue;
        }

        // Score: preferred extension +5, matching arch keyword +10.
        let mut score = 0;
        if lower.ends_with(&prefer_ext.to_lowercase()) {
            score += 5;
        }
        if !right_kw.is_empty() && right_kw.iter().any(|kw| lower.contains(kw)) {
            score += 10;
        }

        match &best {
            Some((bs, _)) if *bs >= score => {}
            _ => best = Some((score, url)),
        }
    }

    best.map(|(_, u)| u)
}

/// Extract the download URL from a single asset object.
/// Uses `browser_download_url` (GitHub, Gitee); falls back to `download_url`.
fn asset_url(asset: &serde_json::Value) -> Option<String> {
    asset
        .get("browser_download_url")
        .or_else(|| asset.get("download_url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Return the current app version (from Cargo.toml).
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Download + verify + install the latest update via `tauri-plugin-updater`,
/// then restart the app. Progress is streamed to the frontend via `on_event`.
///
/// This walks the `latest.json` endpoint (configured in tauri.conf.json →
/// plugins.updater) — independent of `check_update`'s GitHub API path, so the
/// two never interfere. `check_update` decides *whether* an update exists and
/// shows the changelog; `install_update` does the actual replace + relaunch.
///
/// On macOS there's a known restart bug (tauri#11392) where the app may not
/// relaunch — the frontend treats `Installed` as "about to restart" and shows
/// a manual-reopen hint if it's still alive 3s later.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    on_event: Channel<DownloadEvent>,
) -> Result<(), String> {
    let updater = app
        .updater_builder()
        .build()
        .map_err(|e| format!("updater 初始化失败：{e}"))?;

    let update = updater
        .check()
        .await
        .map_err(|e| {
            let msg = format!("检查更新失败：{e}");
            let _ = on_event.send(DownloadEvent::Error {
                message: msg.clone(),
            });
            msg
        })?
        .ok_or_else(|| "当前已是最新版本".to_string())?;

    // Stream download progress to the frontend. v2.10+ API: the first callback
    // fires per chunk with `(chunk_length, total_option)`; the second fires
    // once when download finishes (before install). We synthesize a Started
    // event from the first chunk + its total, then Progress per chunk.
    let first = std::sync::atomic::AtomicBool::new(true);
    let on_progress = {
        let on_event = on_event.clone();
        move |chunk_length: usize, total: Option<u64>| {
            if first.swap(false, std::sync::atomic::Ordering::SeqCst) {
                let _ = on_event.send(DownloadEvent::Started {
                    content_length: total.unwrap_or(0),
                });
            }
            let _ = on_event.send(DownloadEvent::Progress {
                chunk_length: chunk_length as u64,
            });
        }
    };
    let on_download_finish = {
        let on_event = on_event.clone();
        move || {
            let _ = on_event.send(DownloadEvent::Finished);
        }
    };

    update
        .download_and_install(on_progress, on_download_finish)
        .await
        .map_err(|e| {
            let msg = format!("下载/安装失败：{e}");
            let _ = on_event.send(DownloadEvent::Error {
                message: msg.clone(),
            });
            msg
        })?;

    let _ = on_event.send(DownloadEvent::Installed);
    // app.restart() diverges (returns `!`) — the process is replaced.
    app.restart()
}

/// Simple semver comparison: strips "v" prefix and compares major.minor.patch.
/// Returns true when `candidate` is strictly greater than `current`.
///
/// Handles suffixes like `-test`, `-beta`, `-rc1` by extracting only the leading
/// digits from each segment (e.g. `1.0.1-test` → `[1, 0, 1]`).
fn is_newer(current: &str, candidate: &str) -> bool {
    let cur = strip_version(current);
    let cand = strip_version(candidate);
    if cur == cand {
        return false;
    }
    let cur_parts: Vec<u32> = cur
        .split('.')
        .filter_map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        })
        .collect();
    let cand_parts: Vec<u32> = cand
        .split('.')
        .filter_map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        })
        .collect();
    for i in 0..3 {
        let c = cand_parts.get(i).copied().unwrap_or(0);
        let p = cur_parts.get(i).copied().unwrap_or(0);
        if c > p {
            return true;
        }
        if c < p {
            return false;
        }
    }
    // Same major.minor.patch — not newer.
    false
}

fn strip_version(s: &str) -> &str {
    s.strip_prefix('v').unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_prefix_v() {
        assert_eq!(strip_version("v1.2.3"), "1.2.3");
        assert_eq!(strip_version("1.2.3"), "1.2.3");
    }

    #[test]
    fn newer_detected() {
        assert!(is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.0", "1.0.0"));
        assert!(is_newer("0.1.9", "0.1.10"));
    }

    #[test]
    fn not_newer_when_same_or_older() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.2.0", "0.1.0"));
        assert!(!is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn newer_with_suffix_tags() {
        // Suffixes like -test, -beta are stripped; only digits matter.
        assert!(is_newer("1.0.0", "v1.0.1-test"));
        assert!(is_newer("1.0.0", "v1.0.1-beta"));
        assert!(is_newer("1.0.1", "v1.0.2-rc1"));
        assert!(is_newer("0.9.0", "v1.0.0-dev"));
        // Same numeric version with suffix → not newer.
        assert!(!is_newer("1.0.1", "v1.0.1-test"));
    }

    #[test]
    fn pick_asset_prefers_matching_arch() {
        // Use platform-valid extensions so assets aren't filtered by ext check.
        // Architecture keywords differ per OS/arch — wrong-arch assets must be
        // disqualified and the right-arch asset must win.
        let (ext, arm_name, x64_name, arm_url, x64_url): (&str, &str, &str, &str, &str) =
            match (std::env::consts::OS, std::env::consts::ARCH) {
                ("macos", "aarch64") => (".dmg", "arm64", "x64", "arm64.dmg", "x64.dmg"),
                ("macos", "x86_64") => (".dmg", "x64", "arm64", "x64.dmg", "arm64.dmg"),
                ("windows", "x86_64") => (".exe", "x64", "aarch64", "x64.exe", "aarch64.exe"),
                ("windows", "aarch64") => (".exe", "aarch64", "x64", "aarch64.exe", "x64.exe"),
                ("linux", "x86_64") => (
                    ".AppImage",
                    "x64",
                    "aarch64",
                    "x64.AppImage",
                    "aarch64.AppImage",
                ),
                ("linux", "aarch64") => (
                    ".AppImage",
                    "aarch64",
                    "x64",
                    "aarch64.AppImage",
                    "x64.AppImage",
                ),
                _ => (".bin", "a", "b", "a.bin", "b.bin"),
            };

        let body = serde_json::json!({
            "assets": [
                { "name": format!("Token Usage_1.0.0_{}{}", arm_name, ext), "browser_download_url": format!("https://example.com/{}", arm_url) },
                { "name": format!("Token Usage_1.0.0_{}{}", x64_name, ext), "browser_download_url": format!("https://example.com/{}", x64_url) },
            ]
        });

        let url = pick_matching_asset(&body);
        assert!(
            url.is_some(),
            "expected Some(url), got None for os={} arch={}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        assert!(url.unwrap().starts_with("https://example.com/"));
    }

    #[test]
    fn pick_asset_returns_none_when_no_assets() {
        let body = serde_json::json!({ "assets": [] });
        assert_eq!(pick_matching_asset(&body), None);
    }

    #[test]
    fn pick_asset_returns_none_when_missing_assets_key() {
        let body = serde_json::json!({ "tag_name": "v1.0.0" });
        assert_eq!(pick_matching_asset(&body), None);
    }

    #[test]
    fn asset_url_prefers_browser_download_url() {
        let asset = serde_json::json!({
            "browser_download_url": "https://example.com/primary",
            "download_url": "https://example.com/fallback"
        });
        assert_eq!(
            asset_url(&asset),
            Some("https://example.com/primary".into())
        );
    }

    #[test]
    fn asset_url_falls_back_to_download_url() {
        let asset = serde_json::json!({ "download_url": "https://example.com/fallback" });
        assert_eq!(
            asset_url(&asset),
            Some("https://example.com/fallback".into())
        );
    }

    #[test]
    fn parse_repo_github_format() {
        let (o, r) = parse_repo("zechuan/token-usage");
        assert_eq!(o, "zechuan");
        assert_eq!(r, "token-usage");
    }

    #[test]
    fn parse_repo_full_url_format() {
        let (o, r) = parse_repo("github.com/lzc0088/token-usage");
        assert_eq!(o, "lzc0088");
        assert_eq!(r, "token-usage");
    }

    #[test]
    fn parse_repo_owner_repo_format() {
        let (o, r) = parse_repo("lzc0088/token-usage");
        assert_eq!(o, "lzc0088");
        assert_eq!(r, "token-usage");
    }

    #[test]
    fn within_cooldown_1h_window() {
        // 1h cooldown regardless of cached state. 59min → cached.
        assert!(within_cooldown(0, 59 * 60 * 1000));
        // 61min → expired, re-check.
        assert!(!within_cooldown(0, 61 * 60 * 1000));
    }

    #[test]
    fn persist_then_load_roundtrips_last_known_and_timestamp() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();

        let info = UpdateInfo {
            has_update: true,
            version: "v0.2.0".into(),
            name: "Release 0.2.0".into(),
            changelog: "fixes".into(),
            url: "https://example.com/r".into(),
            published_at: Some("2026-08-01T00:00:00Z".into()),
            error: String::new(),
            error_kind: String::new(),
            download_url: Some("https://example.com/asset.dmg".into()),
        };
        persist(&conn, 1_700_000_000_000, Some(&info));

        let (last_check, last_known) = load_cached(&conn);
        assert_eq!(last_check, Some(1_700_000_000_000));
        let known = last_known.expect("last-known should round-trip");
        assert!(known.has_update);
        assert_eq!(known.version, "v0.2.0");
        assert_eq!(known.error_kind, "");
        assert_eq!(
            known.download_url.as_deref(),
            Some("https://example.com/asset.dmg")
        );
    }

    #[test]
    fn persist_records_attempt_without_overwriting_last_known_on_failure() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();

        // Seed a successful last-known.
        let good = UpdateInfo {
            has_update: true,
            version: "v0.2.0".into(),
            name: "".into(),
            changelog: "".into(),
            url: "".into(),
            published_at: None,
            error: String::new(),
            error_kind: String::new(),
            download_url: None,
        };
        persist(&conn, 1_000, Some(&good));

        // A later failed attempt persists the timestamp but NOT a new result.
        persist(&conn, 2_000, None);

        let (last_check, last_known) = load_cached(&conn);
        assert_eq!(last_check, Some(2_000), "attempt timestamp must update");
        // last-known must be preserved (failure must not erase it).
        assert_eq!(last_known.unwrap().version, "v0.2.0");
    }
}
