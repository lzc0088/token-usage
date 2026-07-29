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

use crate::config;
use crate::auth::credentials;
use crate::quota::scheduler;
use crate::quota::{Quota, QuotaBalance, adapter_for};
use crate::state::AppState;

/// Debug command: test credential parsing and API call for a vendor.
/// Returns detailed logs about what happened during the fetch.
#[tauri::command]
pub async fn test_credential(vendor: String, credential: String) -> Result<String, String> {
    #[cfg(debug_assertions)]
    {
        tracing::debug!(vendor = %vendor, len = credential.len(), "test_credential called");
        tracing::debug!(
            "credential (first 100 chars): {}",
            &credential[..credential.len().min(100)]
        );
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
/// A vendor only appears in `quota_cache` if its credential was validated and
/// the quota was fetched successfully, so presence here implies "connected".
#[tauri::command]
pub fn get_quotas(state: State<'_, AppState>) -> Result<Vec<Quota>, String> {
    let conn = state.db_guard();
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
        // Include all cached vendors. `quota_active_vendors` is used for
        // display ordering only — filtering would hide newly-added vendors
        // (e.g. Claude, Codex) from existing configs that predate them.
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
pub async fn refresh_quotas(state: State<'_, AppState>) -> Result<(), String> {
    scheduler::refresh_all(&state.db).await;
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
    let _guard = StaleRefreshGuard;

    // Decide staleness from the freshest cache row + the configured interval.
    let (stale, db) = {
        let conn = state.db_guard();
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
        (
            scheduler::is_stale(min_fetched, interval, now_ms),
            state.db.clone(),
        )
    };

    if !stale {
        return Ok(false);
    }
    scheduler::refresh_all(&db).await;
    let _ = app.emit("quota:updated", ());
    Ok(true)
}

/// Manually trigger a refresh for a single vendor. Updates `quota_cache`
/// and emits `quota:updated` so open pages reload immediately.
#[tauri::command]
pub async fn refresh_quota(vendor: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let cred = {
        let conn = state.db_guard();
        credentials::get(&conn, &vendor).map_err(|e| e.to_string())?
    };
    let (_cfg, now, today, month_start) = {
        let conn = state.db_guard();
        let cfg = config::load(&conn).unwrap_or_default();
        let now = chrono::Utc::now();
        let today = now.format("%Y-%m-%d").to_string();
        let month_start = {
            let (y, m, _d) = (now.year(), now.month(), now.day());
            format!("{y:04}-{m:02}-01")
        };
        (cfg, now, today, month_start)
    };
    let now_rfc = now.to_rfc3339();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let _q = match adapter_for(&vendor) {
        Some(vid) => match crate::quota::fetch(vid, &cred).await {
            Ok(mut q) => {
                q.refreshed_at = Some(now_rfc);
                if let Ok(conn) = state.db.lock() {
                    if q.balance.is_some() {
                        let today_c = scheduler::query_consumption(&conn, &vendor, &today);
                        let month_c = scheduler::query_consumption(&conn, &vendor, &month_start);
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
            Err(e) => {
                tracing::warn!(vendor = %vendor, error = %e, "quota refresh failed");
                return Err(e.to_string());
            }
        },
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
