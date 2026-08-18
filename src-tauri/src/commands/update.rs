//! Update check: query GitHub Releases API for the latest version.
//!
//! Uses `ureq` (blocking) inside an async Tauri command — `ureq` is sync but
//! Tauri runs commands on a blocking thread pool, so this is fine.

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

use std::sync::Arc;

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

/// Build the GitHub Releases API URL for the latest release.
fn api_url(owner: &str, repo: &str) -> String {
    format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        owner, repo
    )
}

/// Normalize a repo string to `owner/repo` before splitting. Callers have
/// historically passed host-prefixed forms ("github.com/owner/repo" from
/// VITE_UPDATE_REPO and the App.svelte default); splitting that on the first
/// '/' yields owner = "github.com" and an API URL that 404s, so released
/// versions were never discovered. Strips an optional scheme + host (any
/// `xxx.tld` first segment), leading/trailing slashes, and whitespace.
fn normalize_repo(repo: &str) -> String {
    let trimmed = repo.trim().trim_matches('/');
    let mut parts: Vec<&str> = trimmed.split('/').filter(|p| !p.is_empty()).collect();
    // Drop leading segments that look like scheme/host parts ("https:",
    // "github.com", "gitee.com") — loop so "https://github.com/…" strips both.
    while parts.len() > 2 && (parts[0].contains('.') || parts[0] == "https:" || parts[0] == "http:")
    {
        parts.remove(0);
    }
    parts.join("/")
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
pub async fn check_update(
    state: State<'_, AppState>,
    repo: String,
    current_version: String,
    force: Option<bool>,
) -> Result<UpdateInfo, String> {
    // Clone the Arc before entering spawn_blocking (closure must be 'static).
    let db_arc: Arc<std::sync::Mutex<rusqlite::Connection>> = state.db.clone();

    tauri::async_runtime::spawn_blocking(move || {
        // Tolerate host-prefixed repo strings ("github.com/owner/repo") so a
        // misconfigured caller can't turn into a 404 API URL.
        let normalized = normalize_repo(&repo);
        let owner = normalized.split_once('/').map(|(o, _)| o).unwrap_or("");
        let repo_name = normalized.split_once('/').map(|(_, r)| r).unwrap_or("");
        let force = force.unwrap_or(false);

        // ── 1. Cooldown short-circuit ──────────────────────────────────────────
        if !force {
            let conn = db_arc.lock().unwrap();
            let (last_check, last_known) = load_cached(&conn);
            if let (Some(last), Some(known)) = (last_check, last_known) {
                let now = now_ms();
                if within_cooldown(last, now) {
                    tracing::debug!("update check skipped: within {}ms cooldown", COOLDOWN_MS);
                    return Ok(known);
                }
            }
        }

        // ── 2. Network call ─────────────────────────────────────────────────────
        let url = api_url(owner, repo_name);

        let resp_result = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(15))
            .call();

        // On any failure: record the attempt timestamp, then return last-known if
        // we have it (don't mislead the UI with has_update:false), else the error.
        let handle_failure =
            |kind: &str, msg: String, last_known: Option<UpdateInfo>| -> UpdateInfo {
                {
                    let conn = db_arc.lock().unwrap();
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
            let conn = db_arc.lock().unwrap();
            load_cached(&conn).1
        };

        let resp = match resp_result {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("网络请求失败：{e}");
                tracing::warn!("update check failed: {msg}");
                return Ok(handle_failure("network", msg, last_known_for_failure));
            }
        };

        if resp.status() != 200 {
            let msg = format!("GitHub API 返回 {}", resp.status());
            tracing::warn!("update check failed: {msg}");
            return Ok(handle_failure("api_error", msg, last_known_for_failure));
        }

        let latest: serde_json::Value = match serde_json::from_reader(resp.into_reader()) {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("响应解析失败：{e}");
                tracing::warn!("update check failed: {msg}");
                return Ok(handle_failure("api_error", msg, last_known_for_failure));
            }
        };

        let tag = latest["tag_name"].as_str().unwrap_or("");
        let name = latest["name"].as_str().unwrap_or("");
        let body = latest["body"].as_str().unwrap_or("");
        let published = latest["published_at"].as_str().map(|s| s.to_string());

        // Normalize: strip leading "v" if present.
        let clean_tag = tag.strip_prefix('v').unwrap_or(tag);
        let has_update = clean_tag != current_version;

        let info = UpdateInfo {
            has_update,
            version: tag.to_string(),
            name: name.to_string(),
            changelog: body.to_string(),
            url: latest["html_url"].as_str().unwrap_or("").to_string(),
            published_at: published,
            error: String::new(),
            error_kind: String::new(),
            download_url: None,
        };

        // Persist successful result.
        {
            let conn = db_arc.lock().unwrap();
            persist(&conn, now_ms(), Some(&info));
        }

        tracing::info!(version = %tag, has_update, "update check complete");
        Ok(info)
    })
    .await
    .map_err(|e| format!("检查更新失败：{e}"))?
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
    // reqwest (used by the updater) reads HTTPS_PROXY/HTTP_PROXY env vars.
    // These are mirrored from the OS system proxy at startup by
    // utils::proxy::sync_system_proxy(), so the updater routes through the
    // user's Clash/V2Ray without any hard-coded port here.
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

    tracing::info!(
        "install_update: found update v{} (target={}) → downloading {}",
        update.version,
        update.target,
        update.download_url
    );

    // Stream download progress to the frontend. v2.10+ API: the first callback
    // fires per chunk with `(chunk_length, total_option)`; the second fires
    // once when download finishes (before install). We synthesize a Started
    // event from the first chunk + its total, then Progress per chunk.
    let first = std::sync::atomic::AtomicBool::new(true);
    let downloaded = std::sync::atomic::AtomicU64::new(0);
    let last_log = std::sync::atomic::AtomicU64::new(0);
    let start = std::time::Instant::now();
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
            // Log download speed every ~1MB so we can tell whether it's going
            // through the proxy / VPN or crawling on a bad direct route.
            let acc = downloaded
                .fetch_add(chunk_length as u64, std::sync::atomic::Ordering::SeqCst)
                + chunk_length as u64;
            let last = last_log.load(std::sync::atomic::Ordering::SeqCst);
            if acc.saturating_sub(last) >= 1024 * 1024 {
                last_log.store(acc, std::sync::atomic::Ordering::SeqCst);
                let secs = start.elapsed().as_secs_f64().max(0.001);
                tracing::info!(
                    "install_update: downloaded {:.2} MB / {:.2} MB ({:.0} KB/s)",
                    acc as f64 / 1_048_576.0,
                    total.map(|t| t as f64 / 1_048_576.0).unwrap_or(0.0),
                    acc as f64 / 1024.0 / secs,
                );
            }
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
    // Don't auto-restart — let the user pick the moment via `restart_app`.
    Ok(())
}

/// Restart the app to finish applying an installed update. Called from the
/// frontend "Restart now" button (`install_update` deliberately does NOT
/// auto-restart, so the user stays in control instead of the app vanishing).
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_cooldown_1h_window() {
        // 1h cooldown regardless of cached state. 59min → cached.
        assert!(within_cooldown(0, 59 * 60 * 1000));
        // 61min → expired, re-check.
        assert!(!within_cooldown(0, 61 * 60 * 1000));
    }

    #[test]
    fn normalize_repo_strips_host_prefixes() {
        // Regression (2026-08-18): callers passed "github.com/owner/repo"
        // (VITE_UPDATE_REPO / App.svelte default), the first-'/' split made
        // owner = "github.com" and every check-update hit
        // api.github.com/repos/github.com/... → 404, so released versions
        // were never discovered.
        assert_eq!(
            normalize_repo("github.com/lzc0088/token-usage"),
            "lzc0088/token-usage"
        );
        assert_eq!(
            normalize_repo("https://github.com/lzc0088/token-usage"),
            "lzc0088/token-usage"
        );
        // Already-normalized and gitee-style hosts pass through untouched.
        assert_eq!(normalize_repo("lzc0088/token-usage"), "lzc0088/token-usage");
        assert_eq!(normalize_repo("gitee.com/owner/repo"), "owner/repo");
        // Leading/trailing slashes and whitespace are tolerated.
        assert_eq!(
            normalize_repo("/lzc0088/token-usage/"),
            "lzc0088/token-usage"
        );
        assert_eq!(
            normalize_repo("  lzc0088/token-usage  "),
            "lzc0088/token-usage"
        );
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
