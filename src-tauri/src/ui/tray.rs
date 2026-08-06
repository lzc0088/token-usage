//! System tray title helper (M6). The TrayIcon itself is created in lib.rs
//! via Tauri's native `tauri::tray::TrayIconBuilder` (integrated with Tauri's
//! event loop, unlike the standalone tray-icon crate). This module formats
//! and dispatches title updates from the collector consumer thread, honouring
//! the user's `tray_display` config.

use rusqlite::Connection;
use serde_json::Value;
use tauri::AppHandle;

use crate::config::{self, Currency};
use crate::query::range_for_period;
use crate::query::summary::{self, Summary};

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
            "{}·{}",
            compact_tokens(s.total_tokens),
            format_cost(s.cost_usd, currency, cny_rate)
        ),
        // icon_only / unknown — caller handles icon_only separately.
        _ => String::new(),
    }
}

/// Format cost per the user's currency config (mirrors frontend `formatCost`).
/// CNY / 双显 use the stored USD→CNY rate, falling back to 7.2 when unset.
fn format_cost(usd: f64, currency: Currency, cny_rate: f64) -> String {
    let rate = if cny_rate > 0.0 { cny_rate } else { 7.2 };
    match currency {
        Currency::Usd => format!("${:.1}", usd),
        Currency::Cny => format!("¥{:.1}", usd * rate),
        Currency::Both => format!("¥{:.1}/${:.1}", usd * rate, usd),
    }
}

fn compact_tokens(n: i64) -> String {
    let abs = n.unsigned_abs();
    if abs >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else if abs >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if abs >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
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
    if mode == "icon_only" {
        // NOTE: tray-icon's macOS `set_title(None)` is a no-op (it only calls
        // setTitle when Some). Pass an empty string to actually clear it.
        let _ = tray.set_title(Some(""));
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
    // Leading thin space: visual breathing room between icon and text.
    let spaced = format!("\u{2009}{title}");
    let _ = tray.set_title(Some(&spaced));
}

/// Update the tray title from a today JSON value emitted by the collector.
pub fn update_from_json(h: &AppHandle, v: &Value, conn: &Connection) {
    if let Some(today) = summary::from_today_json(v) {
        paint(h, conn, &today);
    }
}

/// Repaint the tray title from persisted state alone — used right after a
/// config change, before the next collector tick. Always reaches `paint` so
/// that `icon_only` (which needs no data) clears the title even if the
/// summary query fails.
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
            "2.8M·$4.2"
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
            "2.8M·¥29.5/$4.2"
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
}
