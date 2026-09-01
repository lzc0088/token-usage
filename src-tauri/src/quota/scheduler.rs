//! Quota refresh scheduler (M6 P1).
//!
//! Background task that periodically calls each vendor's quota API and writes
//! the result to `quota_cache`. The frontend reads from cache — no live API
//! calls during normal page loads.
//!
//! Also provides a one-shot refresh for manual "刷新" triggers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Datelike;
use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};
#[cfg(not(target_os = "macos"))]
use tauri_plugin_notification::NotificationExt;
use tokio::time::Duration;
use tracing::{debug, warn};

use std::collections::HashSet;

use crate::auth::credentials;
use crate::config;
use crate::quota::burn_rate::{BurnRateTracker, ADAPTIVE_BASE_SECS};
use crate::quota::{adapter_for, Quota, QuotaBalance, TRACKED_VENDORS};

/// How long to wait before the first scheduled refresh after startup.
const INITIAL_DELAY_MS: u64 = 5_000;
/// Key for persisting the auth-errored (silenced) vendor set in app_config.
const SILENCED_KEY: &str = "quota_auth_errored";

/// Per-vendor fetch timeout. Caps any single vendor's quota API call so a
/// hung/slow endpoint cannot stall the scheduler loop (which would otherwise
/// push the effective refresh period far beyond the configured interval).
/// Some vendors (e.g. Volcengine) issue several internal requests, so 30s
/// gives headroom while still bounding total refresh time.
const VENDOR_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Map vendor IDs to CLI tool names for consumption queries.
/// Map vendor IDs to model name LIKE patterns for consumption queries.
/// Each pattern matches model names in `daily_usage` belonging to that vendor.
/// This is model-based rather than tool-based — a single vendor's models can be
/// consumed by many tools (e.g. DeepSeek models are used by zcode, trae, etc.).
fn vendor_model_patterns(vendor: &str) -> &[&str] {
    match vendor {
        "deepseek" => &["deepseek%"],
        "glm" => &["glm%", "zai%"],
        "kimi" => &["kimi%", "moonshot%"],
        "minimax" => &["minimax%"],
        "volcengine" => &["doubao%", "ep-%"],
        "mimo" => &["mimo%"],
        _ => &[],
    }
}

/// Query total cost_usd from daily_usage for a vendor's models in a period.
/// Uses LIKE patterns so ALL tools using the vendor's models are counted.
pub fn query_consumption(conn: &Connection, vendor: &str, since_date: &str) -> Option<f64> {
    let patterns = vendor_model_patterns(vendor);
    if patterns.is_empty() {
        return None;
    }
    let clauses: Vec<String> = patterns
        .iter()
        .enumerate()
        .map(|(i, _)| format!("model LIKE ?{}", i + 1))
        .collect();
    let sql = format!(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM daily_usage WHERE ({}) AND date >= ?{}",
        clauses.join(" OR "),
        patterns.len() + 1,
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    for p in patterns {
        params.push(Box::new(p.to_string()));
    }
    params.push(Box::new(since_date.to_string()));
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    conn.query_row(&sql, param_refs.as_slice(), |r| r.get::<_, f64>(0))
        .ok()
        .filter(|v| *v > 0.0)
}

/// Write a quota to the quota_cache table.
pub fn write_cache(conn: &Connection, vendor: &str, q: &Quota, fetched_at: i64) {
    if let Ok(data) = serde_json::to_string(q) {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO quota_cache (vendor, data, fetched_at) VALUES (?, ?, ?)",
            rusqlite::params![vendor, data, fetched_at],
        );
    }
}

/// Read all cached quotas from `quota_cache`. Returns `None` on any DB error
/// (best-effort — callers should tolerate silent skip).
fn read_cached_quotas(conn: &Connection) -> Option<Vec<Quota>> {
    let mut stmt = conn.prepare("SELECT data FROM quota_cache").ok()?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).ok()?;
    let mut out = Vec::new();
    for row in rows {
        let data = row.ok()?;
        if let Ok(q) = serde_json::from_str::<Quota>(&data) {
            out.push(q);
        }
    }
    Some(out)
}

/// Evaluate cached quotas against the notification triggers and dispatch
/// system notifications for newly-worthy windows. Best-effort: all failures
/// are logged and silently skipped. Honors the `quota_notify_enabled` config
/// switch; dedups via the shared AppState guard so repeated refreshes don't
/// re-notify for the same window.
pub async fn dispatch_notifications(app: &AppHandle) {
    let state = app.state::<crate::state::AppState>();

    // Read config (switch + language) and the cached quotas in one short lock.
    let (enabled, lang_zh, quotas) = {
        let conn = state.db_read();
        let cfg = config::load(&conn).unwrap_or_default();
        (
            cfg.quota_notify_enabled,
            cfg.language != "en",
            read_cached_quotas(&conn).unwrap_or_default(),
        )
    };
    if !enabled {
        return;
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let Ok(mut dedup) = state.notify_dedup.lock() else {
        return;
    };

    for quota in quotas {
        let candidates = crate::quota::notify::evaluate(&quota, now_ms);
        for cand in candidates {
            if dedup.should_notify(&cand) {
                send_notification(
                    app,
                    crate::quota::notify::build_title(lang_zh),
                    &crate::quota::notify::build_body(&cand, now_ms, lang_zh),
                );
                tracing::debug!(vendor = %cand.vendor, window = %cand.window_label, "notification dispatched");
            }
        }
    }
}

/// Fire one system notification.
///
/// macOS: the tauri-plugin-notification backend (notify-rust →
/// mac-notification-sys) is unusable here — it swizzles
/// `NSBundle.bundleIdentifier` process-wide (breaking keychain/identity reads
/// for the rest of the app's lifetime) and drives the long-removed
/// `NSUserNotificationCenter` API. Spawning `osascript` in a child process is
/// fully isolated: it can never block or corrupt this process.
#[cfg(target_os = "macos")]
fn send_notification(app: &AppHandle, title: &str, body: &str) {
    use crate::quota::notify::escape_applescript;
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        escape_applescript(body),
        escape_applescript(title)
    );
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let _ = app; // AppHandle unused on macOS; kept for signature parity.
}

/// Non-macOS: the plugin's toast (Windows) / dbus (Linux) backends are safe.
#[cfg(not(target_os = "macos"))]
fn send_notification(app: &AppHandle, title: &str, body: &str) {
    // Best-effort: any failure (plugin not ready, permission denied, etc.)
    // is silently skipped — notifications are non-critical.
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Fetch all bound vendors' quotas and cache them. Called for manual refresh.
/// Clears the persisted silenced set so all vendors are retried.
/// Returns false if another refresh was already in progress.
pub async fn refresh_all(state: &crate::state::AppState) -> bool {
    if !try_begin_refresh() {
        debug!("manual refresh skipped — another refresh already in progress");
        return false;
    }
    // Manual refresh: clear the persisted silenced set so all vendors retry.
    let db = state.db.clone();
    if let Ok(conn) = db.lock() {
        let _ = config::set_json(&conn, SILENCED_KEY, &Vec::<String>::new());
    }
    let mut silenced = HashSet::new();
    // Clone the burn tracker so the MutexGuard is not held across an .await
    // (MutexGuard is not Send, which violates Tauri command handler bounds).
    let mut burn = state
        .burn_rate_tracker
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    refresh_all_impl(&db, &mut silenced, None, Some(&mut burn)).await;
    // Merge the updated clone back into the shared tracker.
    if let Ok(mut g) = state.burn_rate_tracker.lock() {
        *g = burn;
    }
    REFRESHING.store(false, Ordering::Release);
    true
}

/// Cookie-based vendors: their credential is a browser session cookie that the
/// user can re-paste from the settings UI at any time (no restart). Such
/// vendors must NOT be permanently silenced on an auth failure — each refresh
/// cycle retries them so a freshly-updated cookie takes effect on the next tick.
fn is_cookie_vendor(id: &str) -> bool {
    matches!(
        id,
        "mimo"
            | "stepfun"
            | "kimi"
            | "iflytek"
            | "qoder"
            | "cursor"
            | "ollama"
            | "opencode"
            | "claude"
            | "codex"
    )
}

fn placeholder(id: &str, auth_failed: bool, now_rfc: &str) -> Quota {
    // Cookie-only vendors (mimo/stepfun/kimi/iflytek) surface an auth failure as
    // `cookie_error` so the frontend can show an inline "update cookie" entry
    // rather than a generic card-wide error.
    let is_cookie_vendor = is_cookie_vendor(id);
    Quota {
        site: None,
        vendor: id.to_string(),
        status: if auth_failed {
            crate::quota::QuotaStatus::Danger
        } else {
            crate::quota::QuotaStatus::Ok
        },
        windows: vec![],
        balance: None,
        plan_label: None,
        // Set refreshed_at so the frontend distinguishes "fetch attempted and
        // failed" from "never fetched". Without it, the card shows
        // "额度读取待实现" instead of the actual error / cookie-error UI.
        refreshed_at: Some(now_rfc.to_string()),
        error: if auth_failed && !is_cookie_vendor {
            Some("凭证已失效，请重新获取".into())
        } else {
            None
        },
        cookie_error: if auth_failed && is_cookie_vendor {
            Some("Cookie 已过期，请重新获取".into())
        } else {
            None
        },
        expires_at: None,
    }
}

/// Parse a `quota_refresh_interval` config value ("1m" | "3m" | "5m" | "10m"
/// | "15m"; also tolerates seconds like "30s") into seconds. Unknown/empty
/// values fall back to the 5-minute default. The special value "adaptive"
/// also parses to its baseline here so staleness checks (which only need a
/// rough freshness bound) work unchanged — the scheduler itself branches on
/// the raw string for the urgency logic.
pub fn parse_interval_secs(raw: &str) -> u64 {
    if raw == "adaptive" {
        return ADAPTIVE_BASE_SECS;
    }
    raw.strip_suffix('m')
        .and_then(|r| r.parse::<u64>().ok().map(|m| m * 60))
        .or_else(|| raw.strip_suffix('s').and_then(|r| r.parse::<u64>().ok()))
        .unwrap_or(300)
}

/// Current unix time in milliseconds.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// True when the freshest cached quota is older than `interval_secs`, or when
/// there is no cache yet. Used to decide whether opening a page should trigger
/// an immediate refresh (vs. letting the scheduler handle it on its next tick).
pub fn is_stale(max_fetched_at_ms: Option<i64>, interval_secs: u64, now_ms: i64) -> bool {
    let last = match max_fetched_at_ms {
        Some(t) if t > 0 => t,
        _ => return true, // no cache yet → treat as stale
    };
    now_ms.saturating_sub(last) > (interval_secs as i64) * 1000
}

/// Prevents concurrent refresh_all_impl executions (e.g. scheduled + manual).
static REFRESHING: AtomicBool = AtomicBool::new(false);

/// Run the quota refresh scheduler in the background.
///
/// Starts after a short delay, then fires every `quota_refresh_interval` (from
/// config, default 5 min). Reads the interval from the DB on each tick so
/// config changes take effect without a restart.
///
/// Adaptive mode (`quota_refresh_interval == "adaptive"`): baseline
/// full refreshes every 5 minutes, PLUS urgency-driven targeted passes that
/// re-probe only the vendors whose windows are burning toward exhaustion
/// (delay = ttl/4, floored at 60s — see `burn_rate`). The schedule only ever
/// shortens the baseline; idle quotas cost nothing extra.
pub async fn run(app: AppHandle, db: Arc<Mutex<Connection>>) {
    // Initial delay to let the app settle.
    tokio::time::sleep(Duration::from_millis(INITIAL_DELAY_MS)).await;

    // Load persisted silenced vendors from DB so auth-errored vendors stay
    // silenced across restarts (avoids futile retries on known-bad API keys).
    let mut auth_errored: HashSet<String> = {
        let conn = db.lock().unwrap_or_else(|e| {
            warn!("db mutex poisoned in quota scheduler startup, recovering: {e}");
            e.into_inner()
        });
        config::get_json::<Vec<String>>(&conn, SILENCED_KEY)
            .ok()
            .flatten()
            .map(|v| v.into_iter().collect())
            .unwrap_or_default()
    };

    // Burn-rate history for adaptive mode. In-memory: restarts reset it and
    // it self-heals after two samples.
    let mut burn = BurnRateTracker::new();
    // When the last FULL (all-vendor) refresh fired. Targeted passes do not
    // advance this — the baseline cadence continues on its own clock.
    let mut last_full_ms = now_ms();

    // First refresh immediately (always full — establishes baselines).
    if try_begin_refresh() {
        refresh_all_impl(&db, &mut auth_errored, None, Some(&mut burn)).await;
        REFRESHING.store(false, Ordering::Release);
    }
    let _ = app.emit("quota:updated", ());
    dispatch_notifications(&app).await;

    loop {
        // Read current config on each loop iteration: the interval (and the
        // tray mode, for the quota_min repaint hook).
        let (interval_raw, tray_mode) = {
            let conn = db.lock().unwrap_or_else(|e| {
                warn!("db mutex poisoned in quota scheduler, recovering: {e}");
                e.into_inner()
            });
            let cfg = config::load(&conn).unwrap_or_default();
            (cfg.quota_refresh_interval.clone(), cfg.tray_display.clone())
        };
        let is_adaptive = interval_raw == "adaptive";
        let base_secs = if is_adaptive {
            ADAPTIVE_BASE_SECS
        } else {
            parse_interval_secs(interval_raw.as_str())
        };

        // Plan the next pass: a full refresh at the baseline cadence, or an
        // earlier targeted probe if an urgent window will burn through before
        // then. Fixed intervals keep the exact previous sleep-then-refresh
        // behaviour.
        let now = now_ms();
        let next_full_ms = last_full_ms.saturating_add((base_secs as i64) * 1000);
        let until_full_ms = (next_full_ms - now).max(1_000);
        let (sleep_ms, do_full) = if !is_adaptive {
            (base_secs * 1000, true)
        } else {
            match burn.urgency(now) {
                Some(u) => {
                    let urgent_ms = (u.delay_secs as i64) * 1000;
                    if urgent_ms < until_full_ms {
                        (urgent_ms.max(1_000) as u64, false)
                    } else {
                        (until_full_ms as u64, true)
                    }
                }
                None => (until_full_ms as u64, true),
            }
        };

        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
        let fired_at = now_ms();

        // Re-read urgency at fire time: a window may have reset while we
        // slept. If the plan was targeted but no urgency remains, skip the
        // pass entirely and re-plan from the baseline on the next iteration.
        let target: Option<Vec<String>> = if do_full {
            None
        } else {
            burn.urgency(fired_at).map(|u| u.vendors)
        };
        let should_run = do_full || target.is_some();

        if should_run {
            if try_begin_refresh() {
                refresh_all_impl(&db, &mut auth_errored, target.as_deref(), Some(&mut burn)).await;
                REFRESHING.store(false, Ordering::Release);
                if do_full {
                    last_full_ms = fired_at;
                }
            } else {
                debug!("skipping scheduled refresh — another refresh already in progress");
            }
            // Notify windows so the "updated" time and quota cards refresh live
            // without the user re-opening the page.
            let _ = app.emit("quota:updated", ());
            dispatch_notifications(&app).await;
            // quota_min tray mode reads the quota cache — repaint so the
            // tightest percentage stays live between collector ticks.
            // Resolve under the lock, apply after releasing: painting while
            // holding the DB lock can deadlock against a sync IPC command on
            // the main thread waiting for the same lock.
            if tray_mode == "quota_min" {
                let job = db
                    .lock()
                    .ok()
                    .and_then(|conn| crate::ui::tray::resolve_refresh_from_db(&app, &conn));
                if let Some(p) = job {
                    crate::ui::tray::apply_paint(&app, p);
                }
            }
        }
        // Persist the updated silenced set so it survives restarts.
        if let Ok(conn) = db.lock() {
            let _ = config::set_json(
                &conn,
                SILENCED_KEY,
                &auth_errored.iter().collect::<Vec<_>>(),
            );
        }
    }
}

/// Try to acquire the refresh lock. Returns true if we got it, false if another
/// refresh is already running.
fn try_begin_refresh() -> bool {
    REFRESHING
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
}

/// Errors that are likely transient and safe to retry (network blips, DNS, TCP
/// resets). Auth errors (401/403) are NOT transient — retrying without new
/// credentials would just spam the endpoint.
fn is_transient_error(e: &crate::quota::VendorError) -> bool {
    use crate::quota::VendorError;
    matches!(e, VendorError::Network(_))
}

/// Internal: refresh vendors, with auth-error silencing.
///
/// `target: None` refreshes every bound vendor (baseline cadence / manual
/// refresh); `Some(ids)` is an adaptive urgency pass that re-probes only the
/// named vendors — a non-urgent vendor's API must not be hammered because a
/// different vendor's window is close to exhaustion. When provided, `burn`
/// records one sample per successfully committed vendor so the next urgency
/// computation measures from this runtime's own probes (never from cached
/// rows, whose timestamps predate this runtime).
///
/// Vendors are fetched **concurrently** (each capped at `VENDOR_FETCH_TIMEOUT`)
/// rather than sequentially, so one slow/hung endpoint cannot stall the whole
/// cycle. Total refresh time ≈ the slowest single vendor (~30s worst case),
/// not the sum of all vendors. DB writes happen sequentially after all fetches
/// resolve, under a single brief lock per vendor.
async fn refresh_all_impl(
    db: &Arc<Mutex<Connection>>,
    silenced: &mut HashSet<String>,
    target: Option<&[String]>,
    mut burn: Option<&mut BurnRateTracker>,
) {
    let (creds, cfg) = {
        let conn = db.lock().unwrap_or_else(|e| {
            warn!("db mutex poisoned in refresh_all_impl, recovering: {e}");
            e.into_inner()
        });
        let cfg = config::load(&conn).unwrap_or_default();
        let creds: Vec<(String, String)> = TRACKED_VENDORS
            .iter()
            .filter_map(|id| {
                // Auto-detect vendors (workbuddy) need no stored credential —
                // their adapter reads the local app session itself.
                if *id == "workbuddy" {
                    return Some(((*id).to_string(), String::new()));
                }
                credentials::get(&conn, id)
                    .ok()
                    .map(|c| ((*id).to_string(), c))
            })
            .collect();
        (creds, cfg)
    };

    let now = chrono::Utc::now();
    let now_rfc = now.to_rfc3339();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let today = now.format("%Y-%m-%d").to_string();
    let month_start = {
        let (y, m, _d) = (now.year(), now.month(), now.day());
        format!("{y:04}-{m:02}-01")
    };

    let active_set = cfg.quota_active_vendors.clone();

    // Build the work list: filter by the adaptive target (if any), by
    // active_vendors, and pre-skip API-key vendors already silenced for a
    // known auth failure (their key won't self-heal, so retrying every cycle
    // only spams logs).
    let work: Vec<(String, String)> = creds
        .into_iter()
        .filter(|(id, _)| {
            if let Some(ids) = target {
                if !ids.contains(id) {
                    return false;
                }
            }
            if let Some(ref active) = active_set {
                if !active.contains(id) {
                    return false;
                }
            }
            // Silenced API-key vendors are skipped; cookie vendors always retry.
            if silenced.contains(id.as_str()) && !is_cookie_vendor(id) {
                return false;
            }
            true
        })
        .collect();

    // Concurrent fetch phase: one task per vendor, each bounded by the timeout.
    // Transient failures (network errors, timeouts) are retried with exponential
    // backoff (1s → 2s → 4s, max 3 retries). Auth errors (401/403) fail fast.
    const MAX_RETRIES: u32 = 3;
    const BASE_BACKOFF_MS: u64 = 1000;
    let mut join_set: tokio::task::JoinSet<FetchOutcome> = tokio::task::JoinSet::new();
    for (id, cred) in work {
        join_set.spawn(async move {
            let Some(vid) = adapter_for(&id) else {
                return FetchOutcome::NoAdapter(id);
            };
            for attempt in 0..=MAX_RETRIES {
                let result =
                    tokio::time::timeout(VENDOR_FETCH_TIMEOUT, crate::quota::fetch(vid, &cred))
                        .await;
                match result {
                    Ok(Ok(q)) => return FetchOutcome::Success(id, q),
                    Ok(Err(e)) => {
                        if attempt < MAX_RETRIES && is_transient_error(&e) {
                            let delay = BASE_BACKOFF_MS * 2u64.pow(attempt);
                            debug!(vendor = %id, attempt, delay_ms = delay, "retrying after transient error: {e}");
                            tokio::time::sleep(Duration::from_millis(delay)).await;
                            continue;
                        }
                        return FetchOutcome::Failed(id, e);
                    }
                    Err(_elapsed) => {
                        debug!(vendor = %id, "quota fetch timed out, will retry next cycle");
                        return FetchOutcome::Failed(
                            id,
                            super::VendorError::Network("fetch timeout".into()),
                        );
                    }
                }
            }
            unreachable!()
        });
    }

    // Sequential write phase: process results as they complete, mutating the
    // silenced set and writing cache rows under short-lived DB locks.
    while let Some(res) = join_set.join_next().await {
        let outcome = match res {
            Ok(o) => o,
            Err(join_err) => {
                warn!(error = %join_err, "quota fetch task panicked");
                continue;
            }
        };
        match outcome {
            FetchOutcome::Success(id, mut q) => {
                silenced.remove(id.as_str());
                q.refreshed_at = Some(now_rfc.clone());
                // Feed the burn-rate tracker BEFORE writing cache so projected
                // exhaustion timestamps are available to persist on each window.
                if let Some(t) = burn.as_deref_mut() {
                    t.record(&id, &q, now_ms);
                    for w in &mut q.windows {
                        w.projected_exhaustion_at = t
                            .projected_exhaustion_ms(&id, &w.label, now_ms)
                            .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
                            .map(|dt| dt.to_rfc3339());
                    }
                }
                if let Ok(conn) = db.lock() {
                    if q.balance.is_some() {
                        let today_c = query_consumption(&conn, &id, &today);
                        let month_c = query_consumption(&conn, &id, &month_start);
                        q.balance = q.balance.map(|b| QuotaBalance {
                            today_consumption: today_c,
                            month_consumption: month_c,
                            ..b
                        });
                    }
                    write_cache(&conn, &id, &q, now_ms);
                }
            }
            FetchOutcome::Failed(id, e) => {
                let is_auth = super::is_auth_error(&e);
                let is_cookie_vendor = is_cookie_vendor(&id);
                // For cookie-based console vendors, an "empty / no usable
                // payload" result almost always means the session cookie
                // is stale — the console API returns HTTP 200 with an
                // empty body instead of a 401. Treat that exactly like an
                // auth failure so the frontend surfaces a cookie_error +
                // "update cookie" entry (same as StepFun's 401 path).
                let empty_means_cookie_fail =
                    is_cookie_vendor && matches!(e, super::VendorError::Empty);
                // Parse errors for cookie vendors also indicate a stale
                // session: the API returns a 200 login-page HTML instead
                // of valid JSON, so parsing fails.
                let parse_means_cookie_fail =
                    is_cookie_vendor && matches!(e, super::VendorError::Parse(_));
                let cookie_fail = is_auth || empty_means_cookie_fail || parse_means_cookie_fail;
                // Cookie-based vendors can have their credentials refreshed
                // from the settings UI at any time, so they are never
                // permanently silenced — every cycle retries them. Only
                // API-key vendors (whose key won't self-heal) are silenced
                // to avoid log spam.
                if is_auth && !is_cookie_vendor {
                    silenced.insert(id.clone());
                }
                if let Ok(conn) = db.lock() {
                    let p = placeholder(&id, cookie_fail, &now_rfc);
                    // fetched_at=0 marks this as a failed attempt so
                    // staleness checks ignore it and retry next time.
                    write_cache(&conn, &id, &p, 0);
                }
            }
            FetchOutcome::NoAdapter(id) => {
                if let Ok(conn) = db.lock() {
                    let p = placeholder(&id, false, &now_rfc);
                    write_cache(&conn, &id, &p, 0);
                }
            }
        }
    }
}

/// Outcome of one vendor's concurrent fetch — collected then written to the DB.
#[allow(clippy::large_enum_variant)]
enum FetchOutcome {
    Success(String, Quota),
    Failed(String, super::VendorError),
    NoAdapter(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_interval_secs_handles_units_and_default() {
        assert_eq!(parse_interval_secs("1m"), 60);
        assert_eq!(parse_interval_secs("5m"), 300);
        assert_eq!(parse_interval_secs("15m"), 900);
        assert_eq!(parse_interval_secs("30s"), 30);
        // Unknown / empty → 5-minute default.
        assert_eq!(parse_interval_secs("bogus"), 300);
        assert_eq!(parse_interval_secs(""), 300);
    }

    #[test]
    fn is_stale_true_when_older_than_interval_or_no_cache() {
        let now = 1_700_000_000_000; // ms
                                     // fetched 10 min ago, interval 5 min → stale
        assert!(is_stale(Some(now - 600_000), 300, now));
        // fetched 1 min ago, interval 5 min → fresh
        assert!(!is_stale(Some(now - 60_000), 300, now));
        // exactly at the interval boundary → not stale (strictly greater-than)
        assert!(!is_stale(Some(now - 300_000), 300, now));
        // no cache → stale
        assert!(is_stale(None, 300, now));
        // zero/garbage fetched_at → stale
        assert!(is_stale(Some(0), 300, now));
    }

    #[test]
    fn cookie_vendors_are_never_silenced() {
        // Cookie vendors' credentials can be updated live from settings, so
        // they must retry every cycle rather than being permanently skipped.
        assert!(is_cookie_vendor("iflytek"));
        assert!(is_cookie_vendor("mimo"));
        assert!(is_cookie_vendor("stepfun"));
        assert!(is_cookie_vendor("kimi"));
        // API-key vendors ARE silenced after an auth failure (key won't self-heal).
        assert!(!is_cookie_vendor("deepseek"));
        assert!(!is_cookie_vendor("glm"));
        assert!(!is_cookie_vendor("minimax"));
        assert!(!is_cookie_vendor("volcengine"));
    }

    #[test]
    fn cookie_vendor_empty_surfaces_as_cookie_error() {
        // A cookie vendor returning Empty (200 with no payload → stale session)
        // must surface a cookie_error so the frontend shows the update entry.
        let p = placeholder("iflytek", true, "2026-01-01T00:00:00+00:00");
        assert!(p.cookie_error.is_some());
        assert!(p.error.is_none());
        // refreshed_at must be set so the frontend knows a fetch was attempted
        // and shows the error/cookie-error UI instead of "额度读取待实现".
        assert!(p.refreshed_at.is_some());
        // Non-cookie vendor with the same flag sets `error`, not cookie_error.
        let p2 = placeholder("deepseek", true, "2026-01-01T00:00:00+00:00");
        assert!(p2.error.is_some());
        assert!(p2.cookie_error.is_none());
        assert!(p2.refreshed_at.is_some());
        // A non-failure (e.g. transient network error) sets neither.
        let p3 = placeholder("iflytek", false, "2026-01-01T00:00:00+00:00");
        assert!(p3.cookie_error.is_none());
        assert!(p3.error.is_none());
        // refreshed_at is still set even for non-auth failures (the fetch was
        // attempted, just didn't succeed — not "never fetched").
        assert!(p3.refreshed_at.is_some());
    }
}
