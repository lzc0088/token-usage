//! Quota refresh scheduler (M6 P1).
//!
//! Background task that periodically calls each vendor's quota API and writes
//! the result to `quota_cache`. The frontend reads from cache — no live API
//! calls during normal page loads.
//!
//! Also provides a one-shot refresh for manual "刷新" triggers.

use std::sync::{Arc, Mutex};

use chrono::Datelike;
use rusqlite::Connection;
use tauri::{AppHandle, Emitter};
use tokio::time::Duration;

use std::collections::HashSet;

use crate::config;
use crate::credentials;
use crate::quota::{Quota, QuotaBalance, VendorId};

/// All vendor ids the account page can bind.
const TRACKED_VENDORS: &[&str] = &[
    "claude",
    "codex",
    "cursor",
    "deepseek",
    "minimax",
    "glm",
    "kimi",
    "volcengine",
    "stepfun",
    "iflytek",
    "copilot",
    "mimo",
    "opencode",
    "zai_team",
    "qoder",
    "ollama",
];

/// How long to wait before the first scheduled refresh after startup.
const INITIAL_DELAY_MS: u64 = 5_000;

/// Map vendor IDs to CLI tool names for consumption queries.
fn vendor_tools(vendor: &str) -> &[&str] {
    match vendor {
        "deepseek" => &["zcode", "deepseek"],
        "glm" => &["zai", "glm"],
        "kimi" => &["kimi"],
        "minimax" => &["minimax"],
        "volcengine" => &["volcengine"],
        "mimo" => &["mimo"],
        _ => &[],
    }
}

/// Query total cost_usd from daily_usage for a vendor's tool names in a period.
pub fn query_consumption(conn: &Connection, vendor: &str, since_date: &str) -> Option<f64> {
    let tools = vendor_tools(vendor);
    if tools.is_empty() {
        return None;
    }
    let placeholders: Vec<String> = tools
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect();
    let sql = format!(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM daily_usage WHERE tool IN ({}) AND date >= ?",
        placeholders.join(",")
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    for t in tools {
        params.push(Box::new(t.to_string()));
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

/// Fetch all bound vendors' quotas and cache them. Called for manual refresh.
/// Uses a fresh silenced set so auth errors retry on manual refresh.
pub async fn refresh_all(db: &Arc<Mutex<Connection>>) {
    let mut silenced = HashSet::new();
    refresh_all_impl(db, &mut silenced).await;
}

fn adapter_for(id: &str) -> Option<VendorId> {
    match id {
        "deepseek" => Some(VendorId::Deepseek),
        "glm" => Some(VendorId::Glm),
        "minimax" => Some(VendorId::Minimax),
        "kimi" => Some(VendorId::Kimi),
        "volcengine" => Some(VendorId::Volcengine),
        "mimo" => Some(VendorId::Mimo),
        "stepfun" => Some(VendorId::Stepfun),
        "iflytek" => Some(VendorId::Iflytek),
        _ => None,
    }
}

/// Cookie-based vendors: their credential is a browser session cookie that the
/// user can re-paste from the settings UI at any time (no restart). Such
/// vendors must NOT be permanently silenced on an auth failure — each refresh
/// cycle retries them so a freshly-updated cookie takes effect on the next tick.
fn is_cookie_vendor(id: &str) -> bool {
    matches!(id, "mimo" | "stepfun" | "kimi" | "iflytek")
}

fn placeholder(id: &str, auth_failed: bool) -> Quota {
    // Cookie-only vendors (mimo/stepfun/kimi/iflytek) surface an auth failure as
    // `cookie_error` so the frontend can show an inline "update cookie" entry
    // rather than a generic card-wide error.
    let is_cookie_vendor = is_cookie_vendor(id);
    Quota {
        vendor: id.to_string(),
        status: if auth_failed {
            crate::quota::QuotaStatus::Danger
        } else {
            crate::quota::QuotaStatus::Ok
        },
        windows: vec![],
        balance: None,
        plan_label: None,
        refreshed_at: None,
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
/// values fall back to the 5-minute default.
pub fn parse_interval_secs(raw: &str) -> u64 {
    raw.strip_suffix('m')
        .and_then(|r| r.parse::<u64>().ok().map(|m| m * 60))
        .or_else(|| raw.strip_suffix('s').and_then(|r| r.parse::<u64>().ok()))
        .unwrap_or(300)
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

/// Run the quota refresh scheduler in the background.
///
/// Starts after a short delay, then fires every `quota_refresh_interval` (from
/// config, default 5 min). Reads the interval from the DB on each tick so
/// config changes take effect without a restart.
pub async fn run(app: AppHandle, db: Arc<Mutex<Connection>>) {
    // Initial delay to let the app settle.
    tokio::time::sleep(Duration::from_millis(INITIAL_DELAY_MS)).await;

    // Track vendors whose last failure was auth-related (401/403). Once silenced,
    // a vendor is skipped until a manual refresh clears the set.
    let mut auth_errored: HashSet<String> = HashSet::new();

    // First refresh immediately.
    refresh_all_impl(&db, &mut auth_errored).await;
    let _ = app.emit("quota:updated", ());

    loop {
        // Read current interval from config on each loop iteration.
        let interval_secs = {
            let conn = db.lock().expect("db poisoned");
            let cfg = config::load(&conn).unwrap_or_default();
            parse_interval_secs(cfg.quota_refresh_interval.as_str())
        };

        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        refresh_all_impl(&db, &mut auth_errored).await;
        // Notify windows so the "updated" time and quota cards refresh live
        // without the user re-opening the page.
        let _ = app.emit("quota:updated", ());
    }
}

/// Internal: refresh all vendors, with auth-error silencing.
async fn refresh_all_impl(db: &Arc<Mutex<Connection>>, silenced: &mut HashSet<String>) {
    let (creds, cfg) = {
        let conn = db.lock().expect("db poisoned");
        let cfg = config::load(&conn).unwrap_or_default();
        let creds: Vec<(String, String)> = TRACKED_VENDORS
            .iter()
            .filter_map(|id| {
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

    let active_set = cfg.quota_active_vendors;

    for (id, cred) in &creds {
        if let Some(ref active) = active_set {
            if !active.contains(id) {
                continue;
            }
        }

        match adapter_for(id) {
            Some(vid) => {
                match crate::quota::fetch(vid, cred).await {
                    Ok(mut q) => {
                        // Success — remove from silenced set and cache.
                        silenced.remove(id.as_str());
                        q.refreshed_at = Some(now_rfc.clone());
                        if let Ok(conn) = db.lock() {
                            if q.balance.is_some() {
                                let today_c = query_consumption(&conn, id, &today);
                                let month_c = query_consumption(&conn, id, &month_start);
                                q.balance = q.balance.map(|b| QuotaBalance {
                                    today_consumption: today_c,
                                    month_consumption: month_c,
                                    ..b
                                });
                            }
                            write_cache(&conn, id, &q, now_ms);
                        }
                    }
                    Err(e) => {
                        let is_auth = super::is_auth_error(&e);
                        let is_cookie_vendor = is_cookie_vendor(id);
                        // For cookie-based console vendors, an "empty / no usable
                        // payload" result almost always means the session cookie
                        // is stale — the console API returns HTTP 200 with an
                        // empty body instead of a 401. Treat that exactly like an
                        // auth failure so the frontend surfaces a cookie_error +
                        // "update cookie" entry (same as StepFun's 401 path).
                        let empty_means_cookie_fail =
                            is_cookie_vendor && matches!(e, super::VendorError::Empty);
                        let cookie_fail = is_auth || empty_means_cookie_fail;
                        // Cookie-based vendors can have their credentials refreshed
                        // from the settings UI at any time, so they are never
                        // permanently silenced — every cycle retries them. Only
                        // API-key vendors (whose key won't self-heal) are silenced
                        // to avoid log spam.
                        if is_auth && !is_cookie_vendor && silenced.contains(id.as_str()) {
                            // Already warned — skip silently.
                            continue;
                        }
                        if is_auth && !is_cookie_vendor {
                            silenced.insert(id.clone());
                        }
                        if let Ok(conn) = db.lock() {
                            let p = placeholder(id, cookie_fail);
                            write_cache(&conn, id, &p, now_ms);
                        }
                    }
                }
            }
            None => {
                if let Ok(conn) = db.lock() {
                    let p = placeholder(id, false);
                    write_cache(&conn, id, &p, now_ms);
                }
            }
        }
    }
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
        let p = placeholder("iflytek", true);
        assert!(p.cookie_error.is_some());
        assert!(p.error.is_none());
        // Non-cookie vendor with the same flag sets `error`, not cookie_error.
        let p2 = placeholder("deepseek", true);
        assert!(p2.error.is_some());
        assert!(p2.cookie_error.is_none());
        // A non-failure (e.g. transient network error) sets neither.
        let p3 = placeholder("iflytek", false);
        assert!(p3.cookie_error.is_none());
        assert!(p3.error.is_none());
    }
}
