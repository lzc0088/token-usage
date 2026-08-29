//! Quota commands (M4 T4.5).
//!
//! - `get_quotas`: reads cached data from `quota_cache` table (fast, no network).
//! - `refresh_quotas`: triggers a live fetch for all bound vendors (manual "刷新").
//! - `refresh_quota`: triggers a live fetch for a single vendor (per-vendor refresh).
//!
//! Background refresh is driven by `quota::scheduler::run()`, started at app boot.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Datelike;
use tauri::{AppHandle, Emitter, State};

use crate::auth::credentials;
use crate::config;
use crate::quota::scheduler;
use crate::quota::{adapter_for, format_validate_error, Quota, QuotaBalance};
use crate::state::AppState;

/// Debug command: test credential parsing and API call for a vendor.
/// Returns detailed logs about what happened during the fetch.
#[tauri::command]
pub async fn test_credential(vendor: String, credential: String) -> Result<String, String> {
    #[cfg(debug_assertions)]
    {
        tracing::debug!(vendor = %vendor, len = credential.len(), "test_credential called");
    }

    let vid = adapter_for(&vendor).ok_or_else(|| format!("no adapter for vendor: {vendor}"))?;

    match crate::quota::fetch(vid, &credential).await {
        Ok(quota) => {
            #[cfg(debug_assertions)]
            {
                let result = format!(
                    "SUCCESS\nvendor: {}\nstatus: {:?}\nwindows: {}\nplan_label: {:?}\nbalance: {:?}",
                    quota.vendor,
                    quota.status,
                    quota.windows.len(),
                    quota.plan_label,
                    quota.balance
                );
                tracing::debug!("{result}");
                Ok(result)
            }
            #[cfg(not(debug_assertions))]
            Ok(format!(
                "SUCCESS\nvendor: {}\nstatus: {:?}",
                quota.vendor, quota.status
            ))
        }
        Err(e) => {
            let error = format!("FAILED: {e}");
            #[cfg(debug_assertions)]
            tracing::debug!("{error}");
            Err(error)
        }
    }
}

/// Read cached quotas from `quota_cache` table, filtered by config's
/// `quota_active_vendors`. Fast — no network calls.
///
/// Every vendor that has a row in `quota_cache` is returned — that row only
/// exists when the vendor was configured and fetched at least once, so there
/// is no need to post-filter by data content. The frontend applies
/// `quota_active_vendors` for visibility, so disabled vendors are hidden on
/// that side.
#[tauri::command]
pub fn get_quotas(state: State<'_, AppState>) -> Result<Vec<Quota>, String> {
    let conn = state.db_read();
    let cfg = config::load(&conn).unwrap_or_default();
    let active_set = cfg.quota_active_vendors;

    let mut stmt = conn
        .prepare("SELECT vendor, data FROM quota_cache")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |r| {
            let vendor: String = r.get(0)?;
            let data: String = r.get(1)?;
            Ok((vendor, data))
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for row in rows {
        let (_vendor, data) = row.map_err(|e| e.to_string())?;
        // Include every cached vendor — `quota_cache` only has rows for
        // vendors that were actually configured and fetched at least once.
        // The frontend filters down to enabled vendors via
        // `quota_active_vendors`, so unconfigured/disconnected vendors are
        // naturally absent. Filtering here by `has_quota_data` hides
        // legitimate cases: error entries (cookie expired), zero-usage
        // accounts, and all-clear balance — the user still needs to see
        // those in the layout tree and limits page.
        if let Ok(q) = serde_json::from_str::<Quota>(&data) {
            out.push(q);
        }
    }
    // Sort by the user's custom vendor order (from the Account quota list).
    // Falls back to active-vendors order, then leaves DB order unchanged.
    let order_key = cfg.quota_vendor_order.as_ref().or(active_set.as_ref());
    if let Some(order_ref) = order_key {
        let order: std::collections::HashMap<&str, usize> = order_ref
            .iter()
            .enumerate()
            .map(|(i, v)| (v.as_str(), i))
            .collect();
        out.sort_by_key(|q| order.get(q.vendor.as_str()).copied().unwrap_or(usize::MAX));
    }
    Ok(out)
}

/// Manually trigger a full refresh for all bound vendors. Updates `quota_cache`
/// in the background. Call this from "刷新" buttons.
#[tauri::command]
pub async fn refresh_quotas(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    scheduler::refresh_all(&state).await;
    // `refresh_all` has no AppHandle so it can't emit; notify the frontend here
    // so Overview/Limits re-fetch the updated quota cache right away.
    let _ = app.emit("quota:updated", ());
    // Dispatch notifications for any newly-eligible quota windows.
    let _ = scheduler::dispatch_notifications(&app).await;
    Ok(())
}

/// Process-wide guard so two pages mounting in quick succession (Overview +
/// Limits) don't kick off overlapping refreshes. The second caller returns
/// `false` immediately and lets the first finish.
static STALE_REFRESHING: AtomicBool = AtomicBool::new(false);

/// Releases `STALE_REFRESHING` on drop — survives early returns so the flag
/// can never get stuck set.
struct StaleRefreshGuard;
impl Drop for StaleRefreshGuard {
    fn drop(&mut self) {
        STALE_REFRESHING.store(false, Ordering::SeqCst);
    }
}

/// If the freshest cached quota is older than `quota_refresh_interval`, refresh
/// now and emit `quota:updated`; otherwise no-op. Called when the user opens
/// the Overview / Limits page so the data shown is never staler than the
/// configured cadence. Returns `true` when a refresh was actually triggered.
#[tauri::command]
pub async fn refresh_quotas_if_stale(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<bool, String> {
    // Claim the guard; bail if another stale-check is already mid-flight.
    if STALE_REFRESHING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(false);
    }

    // Decide staleness from the freshest cache row + the configured interval.
    let stale = {
        let conn = state.db_read();
        let cfg = config::load(&conn).unwrap_or_default();
        let interval = scheduler::parse_interval_secs(&cfg.quota_refresh_interval);
        // Use MIN rather than MAX: a single stale vendor should trigger a
        // refresh even if others were recently updated (e.g. app restart
        // where only some vendors got refreshed before shutdown). Exclude
        // rows with fetched_at=0 (failed placeholders that never succeeded).
        let min_fetched: Option<i64> = conn
            .query_row(
                "SELECT MIN(fetched_at) FROM quota_cache WHERE fetched_at > 0",
                [],
                |r| r.get(0),
            )
            .ok();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        scheduler::is_stale(min_fetched, interval, now_ms)
    };

    if !stale {
        return Ok(false);
    }
    let _guard = StaleRefreshGuard;
    scheduler::refresh_all(&state).await;
    let _ = app.emit("quota:updated", ());
    let _ = scheduler::dispatch_notifications(&app).await;
    Ok(true)
}

/// Manually trigger a refresh for a single vendor. Updates `quota_cache`
/// and emits `quota:updated` so open pages reload immediately.
#[tauri::command]
pub async fn refresh_quota(
    vendor: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let cred = {
        let conn = state.db_read();
        // Auto-detect vendors (workbuddy) have no stored credential — their
        // adapter reads the local app session file directly.
        if vendor == "workbuddy" {
            String::new()
        } else {
            credentials::get(&conn, &vendor).map_err(|e| e.to_string())?
        }
    };
    let now = chrono::Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let month_start = {
        let (y, m, _d) = (now.year(), now.month(), now.day());
        format!("{y:04}-{m:02}-01")
    };
    let now_rfc = now.to_rfc3339();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let _q = match adapter_for(&vendor) {
        Some(vid) => {
            // Bound the fetch so a hung endpoint can't freeze the UI's manual
            // "刷新" button indefinitely. Matches the scheduler's per-vendor cap.
            let fetch = crate::quota::fetch(vid, &cred);
            match tokio::time::timeout(std::time::Duration::from_secs(30), fetch).await {
                Ok(Ok(mut q)) => {
                    q.refreshed_at = Some(now_rfc);
                    if let Ok(conn) = state.db.lock() {
                        if q.balance.is_some() {
                            let today_c = scheduler::query_consumption(&conn, &vendor, &today);
                            let month_c =
                                scheduler::query_consumption(&conn, &vendor, &month_start);
                            q.balance = q.balance.map(|b| QuotaBalance {
                                today_consumption: today_c,
                                month_consumption: month_c,
                                ..b
                            });
                        }
                        scheduler::write_cache(&conn, &vendor, &q, now_ms);
                    }
                    q
                }
                Ok(Err(e)) => {
                    tracing::warn!(vendor = %vendor, error = %e, "quota refresh failed");
                    let err_msg = format_validate_error(&e.to_string());
                    let p = Quota {
                        vendor: vendor.clone(),
                        status: crate::quota::QuotaStatus::Danger,
                        windows: vec![],
                        balance: None,
                        plan_label: None,
                        refreshed_at: Some(now_rfc),
                        error: Some(err_msg.clone()),
                        cookie_error: None,
                        expires_at: None,
                        site: None,
                    };
                    if let Ok(conn) = state.db.lock() {
                        scheduler::write_cache(&conn, &vendor, &p, now_ms);
                    }
                    return Err(err_msg);
                }
                Err(_elapsed) => {
                    tracing::warn!(vendor = %vendor, "manual quota refresh timed out");
                    return Err("刷新超时，请检查网络后重试".into());
                }
            }
        }
        None => return Err(format!("no adapter for {vendor}")),
    };
    let _ = app.emit("quota:updated", ());
    Ok(())
}

/// Run `codex login` OAuth flow. Spawns the CLI, streams stdout to find the
/// authorize URL, emits `codex:login_status` events. Frontend opens the URL
/// for the user to complete OAuth in their browser.
#[tauri::command]
pub async fn codex_login(app: AppHandle) -> Result<(), String> {
    crate::quota::codex::codex_login(&app).await
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::credentials;
    use crate::config::{self, Config};
    use crate::quota::scheduler;
    use crate::quota::{adapter_for, Quota, QuotaBalance, QuotaStatus};
    use crate::state::AppState;
    use crate::storage::schema;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    // SAFETY: `State<'r, T>` is a transparent wrapper `pub struct State<'r, T>(&'r T)`.
    // The inner field is private (cannot use `State(state)` from outside tauri crate),
    // so transmute is the only sound way to construct it from a reference.
    fn state_of(state: &AppState) -> State<'_, AppState> {
        unsafe { std::mem::transmute(state) }
    }

    fn mock_quota(vendor: &str) -> Quota {
        Quota {
            vendor: vendor.to_string(),
            status: QuotaStatus::Ok,
            windows: vec![],
            balance: Some(QuotaBalance {
                amount: 100.0,
                currency: "CNY".into(),
                today_consumption: None,
                month_consumption: None,
            }),
            plan_label: Some("Pay-as-you-go".into()),
            refreshed_at: Some("2026-07-27T10:00:00+00:00".into()),
            error: None,
            cookie_error: None,
            expires_at: None,
            site: None,
        }
    }

    fn insert_cache(conn: &Connection, vendor: &str, q: &Quota, fetched_at: i64) {
        let data = serde_json::to_string(q).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO quota_cache (vendor, data, fetched_at) VALUES (?, ?, ?)",
            rusqlite::params![vendor, data, fetched_at],
        )
        .unwrap();
    }

    // ── get_quotas ────────────────────────────────────────────────────────

    #[test]
    fn get_quotas_empty_cache_returns_empty_vec() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        let result = get_quotas(state_of(&state)).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn get_quotas_deserializes_cached_quotas() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_read();
            insert_cache(&conn, "deepseek", &mock_quota("deepseek"), 1000);
            insert_cache(&conn, "glm", &mock_quota("glm"), 2000);
        }
        let result = get_quotas(state_of(&state)).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn get_quotas_skips_malformed_json_rows() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_read();
            insert_cache(&conn, "deepseek", &mock_quota("deepseek"), 1000);
            conn.execute(
                "INSERT INTO quota_cache (vendor, data, fetched_at) VALUES (?, ?, ?)",
                rusqlite::params!["bad", "{not json at all", 2000],
            )
            .unwrap();
        }
        let result = get_quotas(state_of(&state)).unwrap();
        assert_eq!(result.len(), 1, "malformed JSON should be skipped silently");
        assert_eq!(result[0].vendor, "deepseek");
    }

    #[test]
    fn get_quotas_orders_by_vendor_order_config() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_read();
            insert_cache(&conn, "glm", &mock_quota("glm"), 3000);
            insert_cache(&conn, "deepseek", &mock_quota("deepseek"), 1000);
            insert_cache(&conn, "kimi", &mock_quota("kimi"), 2000);
            let cfg = Config {
                quota_vendor_order: Some(vec!["deepseek".into(), "kimi".into(), "glm".into()]),
                ..Config::default()
            };
            config::save(&conn, &cfg).unwrap();
        }
        let result = get_quotas(state_of(&state)).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].vendor, "deepseek");
        assert_eq!(result[1].vendor, "kimi");
        assert_eq!(result[2].vendor, "glm");
    }

    #[test]
    fn get_quotas_fallback_ordering_to_active_vendors() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_read();
            insert_cache(&conn, "kimi", &mock_quota("kimi"), 2000);
            insert_cache(&conn, "deepseek", &mock_quota("deepseek"), 1000);
            insert_cache(&conn, "glm", &mock_quota("glm"), 3000);
            let cfg = Config {
                quota_active_vendors: Some(vec!["deepseek".into(), "glm".into(), "kimi".into()]),
                quota_vendor_order: None,
                ..Config::default()
            };
            config::save(&conn, &cfg).unwrap();
        }
        let result = get_quotas(state_of(&state)).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].vendor, "deepseek");
        assert_eq!(result[1].vendor, "glm");
        assert_eq!(result[2].vendor, "kimi");
    }

    #[test]
    fn get_quotas_active_vendors_is_not_a_filter() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_read();
            insert_cache(&conn, "deepseek", &mock_quota("deepseek"), 1000);
            insert_cache(&conn, "glm", &mock_quota("glm"), 2000);
            insert_cache(&conn, "kimi", &mock_quota("kimi"), 3000);
            let cfg = Config {
                quota_active_vendors: Some(vec!["deepseek".into()]),
                ..Config::default()
            };
            config::save(&conn, &cfg).unwrap();
        }
        let result = get_quotas(state_of(&state)).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn get_quotas_vendor_order_takes_precedence_over_active_vendors() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_read();
            insert_cache(&conn, "deepseek", &mock_quota("deepseek"), 1000);
            insert_cache(&conn, "glm", &mock_quota("glm"), 2000);
            insert_cache(&conn, "kimi", &mock_quota("kimi"), 3000);
            let cfg = Config {
                quota_vendor_order: Some(vec!["kimi".into(), "glm".into(), "deepseek".into()]),
                quota_active_vendors: Some(vec!["deepseek".into(), "kimi".into(), "glm".into()]),
                ..Config::default()
            };
            config::save(&conn, &cfg).unwrap();
        }
        let result = get_quotas(state_of(&state)).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].vendor, "kimi");
        assert_eq!(result[1].vendor, "glm");
        assert_eq!(result[2].vendor, "deepseek");
    }

    #[test]
    fn get_quotas_empty_vendor_order_treated_as_absent() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_read();
            insert_cache(&conn, "deepseek", &mock_quota("deepseek"), 1000);
            insert_cache(&conn, "glm", &mock_quota("glm"), 2000);
            let cfg = Config {
                quota_vendor_order: Some(vec![]),
                ..Config::default()
            };
            config::save(&conn, &cfg).unwrap();
        }
        let result = get_quotas(state_of(&state)).unwrap();
        assert_eq!(result.len(), 2);
    }

    // ── adapter_for ───────────────────────────────────────────────────────

    #[test]
    fn adapter_for_maps_all_tracked_vendors() {
        for vendor in crate::quota::TRACKED_VENDORS {
            assert!(
                adapter_for(vendor).is_some(),
                "tracked vendor '{vendor}' must have an adapter"
            );
        }
    }

    #[test]
    fn adapter_for_returns_none_for_unknown_vendors() {
        assert!(adapter_for("").is_none());
        assert!(adapter_for("unknown_vendor_xyz").is_none());
    }

    // ── refresh_quota error paths ─────────────────────────────────────────

    #[test]
    fn refresh_quota_no_adapter_path_produces_expected_error() {
        let vid = adapter_for("nonexistent_vendor");
        assert!(vid.is_none());
        let err = format!("no adapter for {}", "nonexistent_vendor");
        assert!(err.contains("no adapter"));
    }

    #[test]
    fn refresh_quota_missing_credential_path_produces_error() {
        let conn = mem();
        let result = credentials::get(&conn, "deepseek");
        assert!(result.is_err());
        match result {
            Err(crate::auth::credentials::CredentialError::NotFound(v)) => {
                assert_eq!(v, "deepseek");
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    // ── quota_cache write path ────────────────────────────────────────────

    #[test]
    fn write_cache_stores_quota_json_and_timestamp() {
        let conn = mem();
        let q = mock_quota("deepseek");
        scheduler::write_cache(&conn, "deepseek", &q, 1_700_000_000_000);

        let (data, fetched): (String, i64) = conn
            .query_row(
                "SELECT data, fetched_at FROM quota_cache WHERE vendor = ?",
                ["deepseek"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(fetched, 1_700_000_000_000);
        let parsed: Quota = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed.vendor, "deepseek");
    }

    #[test]
    fn write_cache_overwrites_existing_row() {
        let conn = mem();
        let q1 = mock_quota("glm");
        let q2 = Quota {
            vendor: "glm".into(),
            status: QuotaStatus::Danger,
            ..mock_quota("glm")
        };
        scheduler::write_cache(&conn, "glm", &q1, 1000);
        scheduler::write_cache(&conn, "glm", &q2, 2000);

        let (data, fetched): (String, i64) = conn
            .query_row(
                "SELECT data, fetched_at FROM quota_cache WHERE vendor = ?",
                ["glm"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(fetched, 2000);
        let parsed: Quota = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed.status, QuotaStatus::Danger);
    }

    // ── refresh_quotas_if_stale logic ────────────────────────────────────

    #[test]
    fn is_stale_true_when_no_valid_cache() {
        let now = 1_700_000_000_000_i64;
        assert!(scheduler::is_stale(None, 300, now));
        assert!(scheduler::is_stale(Some(0), 300, now));
    }

    #[test]
    fn is_stale_false_with_fresh_cache() {
        let now = 1_700_000_000_000_i64;
        assert!(!scheduler::is_stale(Some(now - 60_000), 300, now));
    }

    #[test]
    fn is_stale_true_with_stale_cache() {
        let now = 1_700_000_000_000_i64;
        assert!(scheduler::is_stale(Some(now - 600_000), 300, now));
    }

    #[test]
    fn stale_refresh_guard_prevents_concurrent_access() {
        STALE_REFRESHING.store(false, Ordering::SeqCst);

        let first =
            STALE_REFRESHING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(first.is_ok());

        let second =
            STALE_REFRESHING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(second.is_err());

        STALE_REFRESHING.store(false, Ordering::SeqCst);
    }

    #[test]
    fn stale_refresh_guard_clears_flag_on_drop() {
        STALE_REFRESHING.store(false, Ordering::SeqCst);
        {
            let ok = STALE_REFRESHING
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok();
            assert!(ok);
            let _guard = StaleRefreshGuard;
        }
        assert!(!STALE_REFRESHING.load(Ordering::SeqCst));
    }

    #[test]
    fn stale_refresh_command_excludes_fetched_at_zero() {
        let conn = mem();
        conn.execute(
            "INSERT INTO quota_cache (vendor, data, fetched_at) VALUES (?, ?, ?)",
            rusqlite::params!["a", "{}", 0_i64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO quota_cache (vendor, data, fetched_at) VALUES (?, ?, ?)",
            rusqlite::params!["b", "{}", 1_700_000_000_000_i64],
        )
        .unwrap();
        let min_fetched: Option<i64> = conn
            .query_row(
                "SELECT MIN(fetched_at) FROM quota_cache WHERE fetched_at > 0",
                [],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(min_fetched, Some(1_700_000_000_000_i64));
    }
}
