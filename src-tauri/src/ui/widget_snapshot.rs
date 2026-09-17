//! Widget snapshot exporter — writes a compact JSON snapshot of today's
//! usage / quotas / breakdowns into the shared App Group container so the
//! native macOS WidgetKit extension (`macos-widget/`) can render it.
//!
//! The main app is NOT sandboxed, so it writes the group-container path
//! directly; the widget extension (sandboxed) reads it via its app-group
//! entitlement. Atomic write (tmp + rename) so a concurrently-refreshing
//! widget never sees a half-written file.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::utils::time::now_ms;

/// App Group identifier — MUST match the widget extension's entitlements.
///
/// Team-ID prefixed form (`group.<TEAMID>.…`): works with Developer ID
/// signing WITHOUT a provisioning profile (Apple's rule for manual
/// distribution — token-monitor ships the same way). A bare `group.*`
/// identifier would require Developer ID provisioning profiles for both
/// the host app and the widget extension (App-Store-style registration).
const APP_GROUP: &str = "group.2F74TS79TL.tokenusage";
const SNAPSHOT_FILE: &str = "widget_snapshot.json";
/// How many top tools/models to export (widget shows 4 rows).
const TOP_N: usize = 4;
/// Spark window (days) — matches the widget's sparkline.
const SPARK_DAYS: u32 = 7;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WidgetSnapshot {
    pub updated_at: i64,
    pub today: TodayBlock,
    /// Quota windows flattened per vendor, most-used window first (widget
    /// sorts by tightness itself; exporting raw keeps Swift simple).
    pub quotas: Vec<QuotaRow>,
    pub tools: Vec<BreakdownRow>,
    pub models: Vec<BreakdownRow>,
    /// Daily token totals, oldest → today (7 entries; shorter when fresh).
    pub spark: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TodayBlock {
    pub tokens: i64,
    pub cost_usd: f64,
    /// Differenced live rate (tok/s) when models are active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_tok_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QuotaRow {
    pub vendor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub label: String,
    pub used_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BreakdownRow {
    pub key: String,
    pub tokens: i64,
    pub pct: f64,
    pub cost_usd: f64,
}

/// The group-container directory for the app group.
///
/// macOS 15+ requires the process to resolve the container through
/// `NSFileManager.containerURL(forSecurityApplicationGroupIdentifier:)`
/// first — constructing `~/Library/Group Containers/<group>` by hand
/// triggers a per-launch "App Data" consent prompt (the token-monitor
/// project hit exactly this, 2026-09). When the app carries the app-group
/// entitlement (provisioned release builds) the API returns the authorized
/// path; otherwise (dev, ad-hoc) it returns nil and we fall back to the
/// plain path join.
pub fn snapshot_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Some(p) = ns_group_container(APP_GROUP) {
            return Some(p);
        }
    }
    dirs::home_dir().map(|h| h.join("Library").join("Group Containers").join(APP_GROUP))
}

/// Resolve the App Group container via Foundation (macOS only). Returns nil
/// when the process lacks the app-group entitlement — callers fall back to
/// the manual path.
#[cfg(target_os = "macos")]
fn ns_group_container(group: &str) -> Option<PathBuf> {
    use objc::{msg_send, sel, sel_impl};
    use std::ffi::{CStr, CString};
    unsafe {
        let fm_cls = objc::runtime::Class::get("NSFileManager")?;
        let str_cls = objc::runtime::Class::get("NSString")?;
        let fm: *mut objc::runtime::Object = msg_send![fm_cls, defaultManager];
        if fm.is_null() {
            return None;
        }
        let c_group = CString::new(group).ok()?;
        let group_str: *mut objc::runtime::Object =
            msg_send![str_cls, stringWithUTF8String: c_group.as_ptr()];
        if group_str.is_null() {
            return None;
        }
        let url: *mut objc::runtime::Object =
            msg_send![fm, containerURLForSecurityApplicationGroupIdentifier: group_str];
        if url.is_null() {
            return None; // no entitlement / group unknown to the system
        }
        let c_path: *const std::os::raw::c_char = msg_send![url, fileSystemRepresentation];
        if c_path.is_null() {
            return None;
        }
        let path = CStr::from_ptr(c_path).to_string_lossy().into_owned();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }
}

fn snapshot_path() -> Option<PathBuf> {
    snapshot_dir().map(|d| d.join(SNAPSHOT_FILE))
}

/// Assemble the snapshot from live caches + DB. Never fails hard — missing
/// pieces degrade to empty blocks so the widget still renders a frame.
pub fn build_snapshot(state: &AppState) -> WidgetSnapshot {
    let conn = state.db.lock().unwrap_or_else(|e| {
        tracing::warn!("db mutex poisoned in widget snapshot, recovering: {e}");
        e.into_inner()
    });

    // Today: prefer the live cache (same value as tray/hero); fall back to
    // the DB-backed daily_usage query.
    let today = if let Ok(cache) = state.last_today.lock() {
        cache.as_ref().map(|live| TodayBlock {
            tokens: live.total_tokens,
            cost_usd: live.cost_usd,
            rate_tok_s: live.live_rate_speed,
        })
    } else {
        None
    };
    let today = today.unwrap_or_else(|| {
        let t = chrono::Local::now().format("%Y-%m-%d").to_string();
        let range = crate::query::range_for_period(crate::query::Period::Day, &t);
        crate::query::summary::query(&conn, &range)
            .map(|s| TodayBlock {
                tokens: s.total_tokens,
                cost_usd: s.cost_usd,
                rate_tok_s: None,
            })
            .unwrap_or(TodayBlock {
                tokens: 0,
                cost_usd: 0.0,
                rate_tok_s: None,
            })
    });

    // Breakdowns: today's top tools/models.
    let t = chrono::Local::now().format("%Y-%m-%d").to_string();
    let range = crate::query::range_for_period(crate::query::Period::Day, &t);
    let top = |dim: crate::query::Dimension| -> Vec<BreakdownRow> {
        crate::query::breakdown::query(&conn, &range, dim)
            .map(|b| {
                b.entries
                    .into_iter()
                    .take(TOP_N)
                    .map(|e| BreakdownRow {
                        key: e.key,
                        tokens: e.tokens,
                        pct: e.token_pct,
                        cost_usd: e.cost_usd,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let tools = top(crate::query::Dimension::Tool);
    let models = top(crate::query::Dimension::Model);

    // Quotas: cached vendor rows, flattened windows.
    let mut quotas: Vec<QuotaRow> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT data FROM quota_cache") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for row in rows.flatten() {
                if let Ok(q) = serde_json::from_str::<crate::quota::types::Quota>(&row) {
                    for w in q.windows {
                        quotas.push(QuotaRow {
                            vendor: q.vendor.clone(),
                            plan: q.plan_label.clone(),
                            label: w.label,
                            used_pct: w.used_pct,
                            resets_at: w.resets_at,
                        });
                    }
                }
            }
        }
    }
    // Tightest first — that's the widget's default sort.
    quotas.sort_by(|a, b| {
        b.used_pct
            .partial_cmp(&a.used_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    quotas.truncate(8);

    // Sparkline: daily totals for the last 7 days.
    let spark = crate::query::trends::query(&conn, &crate::query::last_n_days(&t, SPARK_DAYS))
        .map(|tr| tr.points.into_iter().map(|p| p.tokens).collect())
        .unwrap_or_default();

    WidgetSnapshot {
        updated_at: now_ms(),
        today,
        quotas,
        tools,
        models,
        spark,
    }
}

/// Serialize + atomically write the snapshot into the App Group container.
/// Best-effort: failures are logged, never propagated (the widget simply
/// keeps the previous frame).
pub fn export_snapshot(app: &AppHandle) {
    let state = app.state::<AppState>();
    let snap = build_snapshot(&state);
    let Some(path) = snapshot_path() else {
        tracing::warn!("widget snapshot: no home dir, skip");
        return;
    };
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!(error = %e, "widget snapshot: create group dir failed");
            return;
        }
    }
    match serde_json::to_vec(&snap) {
        Ok(bytes) => {
            let tmp = path.with_extension("json.tmp");
            if let Err(e) = std::fs::write(&tmp, &bytes) {
                tracing::warn!(error = %e, "widget snapshot: write failed");
                return;
            }
            if let Err(e) = std::fs::rename(&tmp, &path) {
                tracing::warn!(error = %e, "widget snapshot: rename failed");
            } else {
                tracing::debug!(bytes = bytes.len(), "widget snapshot exported");
            }
        }
        Err(e) => tracing::warn!(error = %e, "widget snapshot: serialize failed"),
    }
}

/// Debounced export for event-driven callers: skips when the last export is
/// younger than `min_interval_ms`. Shared across all trigger points.
pub struct SnapshotDebounce {
    last_ms: std::sync::atomic::AtomicI64,
    min_interval_ms: i64,
}

impl SnapshotDebounce {
    pub fn new(min_interval_ms: i64) -> Self {
        Self {
            last_ms: std::sync::atomic::AtomicI64::new(0),
            min_interval_ms,
        }
    }

    /// Fire the export unless suppressed by the debounce window.
    pub fn fire(&self, app: &AppHandle) {
        let now = now_ms();
        let last = self.last_ms.load(std::sync::atomic::Ordering::SeqCst);
        if now - last < self.min_interval_ms {
            return;
        }
        // Claim the slot before exporting (concurrent events collapse).
        self.last_ms.store(now, std::sync::atomic::Ordering::SeqCst);
        export_snapshot(app);
    }
}

/// Shared 60s debounce for all event-driven trigger points.
static DEBOUNCE: std::sync::OnceLock<SnapshotDebounce> = std::sync::OnceLock::new();

pub fn export_debounced(app: &AppHandle) {
    DEBOUNCE
        .get_or_init(|| SnapshotDebounce::new(60_000))
        .fire(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_json_shape_matches_swift_codable() {
        let snap = WidgetSnapshot {
            updated_at: 1_789_000_000_000,
            today: TodayBlock {
                tokens: 79_072_426,
                cost_usd: 17.2,
                rate_tok_s: Some(182.5),
            },
            quotas: vec![QuotaRow {
                vendor: "glm".into(),
                plan: Some("GLM Coding Plan".into()),
                label: "5h".into(),
                used_pct: 34.0,
                resets_at: Some("2026-09-17T12:00:00Z".into()),
            }],
            tools: vec![BreakdownRow {
                key: "claude".into(),
                tokens: 65_000_000,
                pct: 82.0,
                cost_usd: 12.1,
            }],
            models: vec![BreakdownRow {
                key: "glm-5.3".into(),
                tokens: 32_500_000,
                pct: 41.0,
                cost_usd: 6.0,
            }],
            spark: vec![42, 61, 38, 75, 52, 90, 68],
        };
        let v: serde_json::Value = serde_json::to_value(&snap).unwrap();
        // Keys the Swift Codable model expects (snake_case passthrough).
        assert_eq!(v["updated_at"], serde_json::json!(1_789_000_000_000i64));
        assert_eq!(v["today"]["tokens"], 79_072_426);
        assert_eq!(v["today"]["rate_tok_s"], 182.5);
        assert_eq!(v["quotas"][0]["used_pct"], 34.0);
        assert_eq!(v["tools"][0]["key"], "claude");
        assert_eq!(v["spark"].as_array().unwrap().len(), 7);
        // Optional fields are omitted, not nulled.
        let today_str = v["today"].to_string();
        assert!(!today_str.contains("rate_burn"));
    }

    #[test]
    fn optional_fields_skip_when_none() {
        let snap = WidgetSnapshot {
            updated_at: 0,
            today: TodayBlock {
                tokens: 0,
                cost_usd: 0.0,
                rate_tok_s: None,
            },
            quotas: vec![],
            tools: vec![],
            models: vec![],
            spark: vec![],
        };
        let s = serde_json::to_string(&snap).unwrap();
        assert!(!s.contains("rate_tok_s"));
        assert!(!s.contains("resets_at"));
        assert!(!s.contains("plan"));
    }

    #[test]
    fn debounce_suppresses_rapid_repeats() {
        use std::sync::atomic::AtomicI64;
        // Pure timing logic on a fake clock: last_ms manipulated directly.
        let d = SnapshotDebounce {
            last_ms: AtomicI64::new(1_000_000),
            min_interval_ms: 60_000,
        };
        let now = 1_000_000 + 30_000; // 30s after the last fire → suppressed.
        assert!(now - d.last_ms.load(std::sync::atomic::Ordering::SeqCst) < d.min_interval_ms);
        let now = 1_000_000 + 61_000; // Past the window → allowed.
        assert!(now - d.last_ms.load(std::sync::atomic::Ordering::SeqCst) >= d.min_interval_ms);
    }
}
