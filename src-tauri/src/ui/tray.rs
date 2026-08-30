//! System tray title helper (M6). The TrayIcon itself is created in lib.rs
//! via Tauri's native `tauri::tray::TrayIconBuilder`. This module formats
//! usage titles and dispatches them from the collector consumer thread,
//! honouring the user's `tray_display` config.
//!
//! The title is rendered INTO the icon bitmap (`ui::tray_icon`), not via
//! `set_title`: Tauri offers no title-font control on macOS and no title
//! support at all on Windows, whereas a bitmap scales to the menu-bar height
//! on every platform — the text renders ~40% smaller than the system font.

use rusqlite::Connection;
use serde_json::Value;
use tauri::AppHandle;

use crate::config::{self, Currency};
use crate::query::range_for_period;
use crate::query::summary::{self, Summary};
use crate::ui::fmt::{compact_tokens, format_cost};

/// Tray display modes that read the "total" range instead of today.
fn is_total_mode(mode: &str) -> bool {
    mode.starts_with("total_")
}

/// Today's date as `YYYY-MM-DD`. `range_for_period` debug-asserts this shape
/// for every period (including Total), so callers must never pass "".
fn today_str() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Format a summary into a compact tray title according to `mode`
/// (one of the `tray_display` config values). Empty string for `icon_only`.
fn format_title(s: &Summary, mode: &str, currency: Currency, cny_rate: f64) -> String {
    match mode {
        "today_tokens" | "total_tokens" => compact_tokens(s.total_tokens),
        "today_cost" | "total_cost" => format_cost(s.cost_usd, currency, cny_rate),
        "today_both" | "total_both" => format!(
            "{}/{}",
            compact_tokens(s.total_tokens),
            format_cost(s.cost_usd, currency, cny_rate)
        ),
        // icon_only / quota_min / unknown — handled by the caller.
        _ => String::new(),
    }
}

/// The "closest-to-exhaustion" quota percentage across all cached vendors —
/// the single number a multi-account user most wants pinned to the menu bar.
/// Reads only successfully-fetched rows (`fetched_at > 0`; failed attempts
/// write 0 and carry placeholder windows), and considers window-level
/// `used_pct` (the aggregate a quota card renders — sub-item detail stays in
/// the popover). Empty when no usable quota data exists.
fn quota_min_title(conn: &Connection) -> String {
    let rows = match conn.prepare("SELECT data FROM quota_cache WHERE fetched_at > 0") {
        Ok(mut stmt) => {
            let it = stmt.query_map([], |r| r.get::<_, String>(0));
            match it {
                Ok(rows) => rows.filter_map(|r| r.ok()).collect::<Vec<_>>(),
                Err(_) => return String::new(),
            }
        }
        Err(_) => return String::new(),
    };
    let mut worst: Option<f64> = None;
    for data in rows {
        let Ok(q) = serde_json::from_str::<crate::quota::Quota>(&data) else {
            continue;
        };
        for w in &q.windows {
            // Clamp defensively: vendor adapters may emit >100 briefly.
            let pct = w.used_pct.clamp(0.0, 100.0);
            if pct > 0.0 && worst.map_or(true, |cur| pct > cur) {
                worst = Some(pct);
            }
        }
    }
    worst
        .map(|pct| format!("{}%", pct.round() as u32))
        .unwrap_or_default()
}

/// Load the `tray_display` config, defaulting to `today_both` on error.
fn load_mode(conn: &Connection) -> String {
    config::load(conn)
        .map(|c| c.tray_display)
        .unwrap_or_else(|e| {
            tracing::warn!("tray config load failed: {e}");
            String::from("today_both")
        })
}

/// Latest stored USD→CNY rate (any date). Falls back to 7.2 when the table is
/// empty or the query fails — matches the frontend's getLatestRate default.
fn load_cny_rate(conn: &Connection) -> f64 {
    conn.query_row(
        "SELECT rate FROM exchange_rate ORDER BY date DESC LIMIT 1",
        [],
        |r| r.get::<_, f64>(0),
    )
    .unwrap_or(7.2)
}

/// Paint the tray title from an already-resolved today summary + the current
/// config mode. Shared by the realtime and DB-paint code paths.
fn paint(h: &AppHandle, conn: &Connection, today: &Summary) {
    let mode = load_mode(conn);
    let currency = config::load(conn)
        .map(|c| c.currency)
        .unwrap_or(Currency::Both);
    let cny_rate = load_cny_rate(conn);
    let Some(tray) = h.tray_by_id("main") else {
        return;
    };

    // Clear any legacy native title (upgrades from set_title-based builds),
    // then paint the title into the icon bitmap.
    let _ = tray.set_title(Some(""));

    // icon_only: plain T glyph, no text region.
    if mode == "icon_only" {
        let icon = crate::ui::tray_icon::build_tray_icon();
        let _ = tray.set_icon_with_as_template(Some(icon), true);
        return;
    }

    // quota_min: the tightest cached quota percentage — independent of the
    // usage summary paths. Repainted by the quota scheduler after each cycle.
    if mode == "quota_min" {
        let title = quota_min_title(conn);
        let icon = crate::ui::tray_icon::build_icon_with_text(&title);
        let _ = tray.set_icon_with_as_template(Some(icon), true);
        return;
    }

    // total_* modes query the unbounded range; today_* use the passed summary.
    let s = if is_total_mode(&mode) {
        match summary::query(
            conn,
            &range_for_period(crate::query::Period::Total, &today_str()),
        ) {
            Ok(total) => total,
            Err(e) => {
                tracing::warn!("tray total summary query failed: {e}");
                today.clone()
            }
        }
    } else {
        today.clone()
    };
    let title = format_title(&s, &mode, currency, cny_rate);
    let icon = crate::ui::tray_icon::build_icon_with_text(&title);
    let _ = tray.set_icon_with_as_template(Some(icon), true);
}

/// Update the tray title from a today JSON value emitted by the collector.
pub fn update_from_json(h: &AppHandle, v: &Value, conn: &Connection) {
    if let Some(today) = summary::from_today_json(v) {
        paint(h, conn, &today);
    }
}

/// Repaint the tray title from persisted state alone — used right after a
/// config change, before the next collector tick.
pub fn refresh_from_db(h: &AppHandle, conn: &Connection) {
    let today = summary::query(
        conn,
        &range_for_period(crate::query::Period::Day, &today_str()),
    )
    .unwrap_or_else(|_| zero_summary());
    paint(h, conn, &today);
}

/// A zeroed-out summary used when the DB query fails — enough to render a
/// title (or clear it for `icon_only`).
fn zero_summary() -> Summary {
    Summary {
        period: "day".into(),
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
        total_tokens: 0,
        cost_usd: 0.0,
        messages: 0,
        delta_pct: None,
        delta_label: None,
        timed_output_tokens: None,
        timed_tokens: None,
        timed_duration_ms: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Summary {
        Summary {
            period: "day".into(),
            total_tokens: 2_840_000,
            cost_usd: 4.21,
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
            messages: 0,
            delta_pct: None,
            delta_label: None,
            timed_output_tokens: None,
            timed_tokens: None,
            timed_duration_ms: None,
        }
    }

    #[test]
    fn format_examples() {
        let s = sample();
        // USD
        assert_eq!(
            format_title(&s, "today_both", Currency::Usd, 7.2),
            "2.8M/$4.2"
        );
        assert_eq!(format_title(&s, "today_cost", Currency::Usd, 7.2), "$4.2");
        assert_eq!(format_title(&s, "today_tokens", Currency::Usd, 7.2), "2.8M");
        // CNY (rate 7.0 → 4.21 * 7.0 = 29.47 → ¥29.5)
        assert_eq!(format_title(&s, "today_cost", Currency::Cny, 7.0), "¥29.5");
        // 双显
        assert_eq!(
            format_title(&s, "today_cost", Currency::Both, 7.0),
            "¥29.5/$4.2"
        );
        assert_eq!(
            format_title(&s, "total_both", Currency::Both, 7.0),
            "2.8M/¥29.5/$4.2"
        );
        assert_eq!(format_title(&s, "icon_only", Currency::Usd, 7.2), "");
    }

    #[test]
    fn total_mode_detected() {
        assert!(is_total_mode("total_tokens"));
        assert!(is_total_mode("total_both"));
        assert!(!is_total_mode("today_tokens"));
        assert!(!is_total_mode("icon_only"));
    }

    #[test]
    fn quota_min_picks_worst_window_across_vendors() {
        use crate::quota::{Quota, QuotaStatus, QuotaWindow};
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE quota_cache (vendor TEXT PRIMARY KEY, data TEXT NOT NULL, fetched_at INTEGER NOT NULL)",
            [],
        )
        .unwrap();
        let q = |used: &[f64]| Quota {
            site: None,
            vendor: "v".into(),
            status: QuotaStatus::Ok,
            windows: used
                .iter()
                .map(|u| QuotaWindow {
                    label: "5h".into(),
                    used_pct: *u,
                    ..Default::default()
                })
                .collect(),
            balance: None,
            plan_label: None,
            refreshed_at: None,
            error: None,
            cookie_error: None,
            expires_at: None,
        };
        // glm 42%/78%, codex 83% → "83%".
        conn.execute(
            "INSERT INTO quota_cache VALUES ('glm', ?1, 1000)",
            rusqlite::params![serde_json::to_string(&q(&[42.0, 78.0])).unwrap()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO quota_cache VALUES ('codex', ?1, 1000)",
            rusqlite::params![serde_json::to_string(&q(&[83.0])).unwrap()],
        )
        .unwrap();
        // Failed fetch (fetched_at=0) carries a placeholder — must be ignored.
        conn.execute(
            "INSERT INTO quota_cache VALUES ('kimi', ?1, 0)",
            rusqlite::params![serde_json::to_string(&q(&[99.0])).unwrap()],
        )
        .unwrap();
        assert_eq!(quota_min_title(&conn), "83%");
    }

    #[test]
    fn quota_min_empty_when_no_usable_data() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE quota_cache (vendor TEXT PRIMARY KEY, data TEXT NOT NULL, fetched_at INTEGER NOT NULL)",
            [],
        )
        .unwrap();
        // Empty table, and a quota with no windows, both render nothing.
        assert_eq!(quota_min_title(&conn), "");
    }

    #[test]
    fn quota_min_clamps_overshoot_and_rounds() {
        use crate::quota::{Quota, QuotaStatus, QuotaWindow};
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE quota_cache (vendor TEXT PRIMARY KEY, data TEXT NOT NULL, fetched_at INTEGER NOT NULL)",
            [],
        )
        .unwrap();
        let q = Quota {
            site: None,
            vendor: "v".into(),
            status: QuotaStatus::Ok,
            windows: vec![QuotaWindow {
                label: "5h".into(),
                used_pct: 101.7, // vendor overshoot — clamped to 100
                ..Default::default()
            }],
            balance: None,
            plan_label: None,
            refreshed_at: None,
            error: None,
            cookie_error: None,
            expires_at: None,
        };
        conn.execute(
            "INSERT INTO quota_cache VALUES ('v', ?1, 1000)",
            rusqlite::params![serde_json::to_string(&q).unwrap()],
        )
        .unwrap();
        assert_eq!(quota_min_title(&conn), "100%");
    }
}
