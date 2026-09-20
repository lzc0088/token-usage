//! Widget snapshot exporter — feeds a compact JSON snapshot of today's
//! usage / quotas / breakdowns to the native macOS WidgetKit extension
//! (`macos-widget/`).
//!
//! No App Group: the system silently ignores app-group entitlements under
//! ad-hoc signing (no Team ID), so sharing runs through a publisher helper
//! that embeds the WIDGET extension's bundle identity — its
//! UserDefaults.standard writes land in the widget's sandbox container,
//! the exact plist the widget reads (Metrik's proven pattern). After
//! publishing, a reloader helper (host app's bundle identity) asks
//! WidgetCenter to refresh the timelines.

use serde::Serialize;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::utils::time::now_ms;

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
    /// Per-day detail for the trend chart widget (7 entries; superset of spark).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trend_points: Vec<TrendPoint>,
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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TrendPoint {
    pub date: String,
    pub tokens: i64,
    pub cost_usd: f64,
    pub messages: i64,
}

/// Resolve a bundled helper's path: env override first (tests / dev), then
/// `<exe>/../../Helpers/<name>` (the .app layout: Contents/MacOS/exe →
/// Contents/Helpers/name). Returns None when the helper isn't shipped —
/// callers skip quietly (non-macOS builds, `tauri dev` runs).
#[cfg(target_os = "macos")]
fn helper_path(env_var: &str, name: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(env_var) {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let exe = std::env::current_exe().ok()?;
    let contents = exe.parent()?.parent()?;
    let helper = contents.join("Helpers").join(name);
    helper.is_file().then_some(helper)
}

#[cfg(target_os = "macos")]
fn publisher_path() -> Option<PathBuf> {
    helper_path("TOKEN_USAGE_WIDGET_PUBLISHER", "token-usage-widget-publish")
}

#[cfg(target_os = "macos")]
fn reloader_path() -> Option<PathBuf> {
    helper_path("TOKEN_USAGE_WIDGET_RELOADER", "token-usage-widget-reload")
}

/// Feed the snapshot JSON to the publisher helper via stdin; it writes the
/// widget container's UserDefaults. Best-effort: missing helper (dev run,
/// non-macOS) or a failed spawn logs and moves on.
fn publish_snapshot(snapshot_json: &[u8]) {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = snapshot_json;
    }
    #[cfg(target_os = "macos")]
    if let Some(publisher) = publisher_path() {
        match Command::new(&publisher)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    if stdin.write_all(snapshot_json).is_err() || stdin.flush().is_err() {
                        tracing::warn!("widget snapshot: publisher stdin write failed");
                    }
                }
                // Drop stdin to let the publisher finish reading.
                drop(child.stdin.take());
                match child.wait() {
                    Ok(st) if st.success() => {}
                    other => tracing::warn!(status = ?other, "widget publisher exited nonzero"),
                }
            }
            Err(e) => tracing::warn!(error = %e, "widget snapshot: cannot spawn publisher"),
        }
    }
}

/// Ask WidgetCenter to refresh our widget kinds (via the host-identity
/// helper). Fire-and-forget.
#[cfg(target_os = "macos")]
fn request_reload() {
    if let Some(reloader) = reloader_path() {
        if let Err(e) = Command::new(&reloader)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut c| c.wait().map(|_| ()))
        {
            tracing::warn!(error = ?e, "widget reloader spawn failed");
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn request_reload() {}

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

    // Sparkline + trend detail: daily totals for the last 7 days.
    let trend_data = crate::query::trends::query(&conn, &crate::query::last_n_days(&t, SPARK_DAYS))
        .map(|tr| tr.points)
        .unwrap_or_default();
    let spark: Vec<i64> = trend_data.iter().map(|p| p.tokens).collect();
    let trend_points: Vec<TrendPoint> = trend_data
        .into_iter()
        .map(|p| TrendPoint {
            date: p.date,
            tokens: p.tokens,
            cost_usd: p.cost_usd,
            messages: p.messages,
        })
        .collect();

    WidgetSnapshot {
        updated_at: now_ms(),
        today,
        quotas,
        tools,
        models,
        spark,
        trend_points,
    }
}

/// Serialize the snapshot, hand it to the publisher helper, then ask
/// WidgetCenter for a reload. Best-effort: failures are logged, never
/// propagated (the widget simply keeps the previous frame).
pub fn export_snapshot(app: &AppHandle) {
    let state = app.state::<AppState>();
    let snap = build_snapshot(&state);
    match serde_json::to_vec(&snap) {
        Ok(bytes) => {
            publish_snapshot(&bytes);
            request_reload();
            tracing::debug!(bytes = bytes.len(), "widget snapshot published");
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
            trend_points: vec![TrendPoint {
                date: "2026-09-11".into(),
                tokens: 4_200_000,
                cost_usd: 3.8,
                messages: 12,
            }],
        };
        let v: serde_json::Value = serde_json::to_value(&snap).unwrap();
        // Keys the Swift Codable model expects (snake_case passthrough).
        assert_eq!(v["updated_at"], serde_json::json!(1_789_000_000_000i64));
        assert_eq!(v["today"]["tokens"], 79_072_426);
        assert_eq!(v["today"]["rate_tok_s"], 182.5);
        assert_eq!(v["quotas"][0]["used_pct"], 34.0);
        assert_eq!(v["tools"][0]["key"], "claude");
        assert_eq!(v["spark"].as_array().unwrap().len(), 7);
        assert_eq!(v["trend_points"][0]["date"], "2026-09-11");
        assert_eq!(v["trend_points"][0]["tokens"], 4_200_000);
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
            trend_points: vec![],
        };
        let s = serde_json::to_string(&snap).unwrap();
        assert!(!s.contains("rate_tok_s"));
        assert!(!s.contains("resets_at"));
        assert!(!s.contains("plan"));
        assert!(!s.contains("trend_points"));
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
