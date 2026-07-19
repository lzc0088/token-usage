//! System tray title helper (M6). The TrayIcon itself is created in lib.rs
//! via Tauri's native `tauri::tray::TrayIconBuilder` (integrated with Tauri's
//! event loop, unlike the standalone tray-icon crate). This module only formats
//! and dispatches title updates from the collector consumer thread.

use serde_json::Value;
use tauri::AppHandle;

use crate::query::summary::{self, Summary};

/// Format the today summary into a compact tray title (e.g. "2.84M · $4.21").
pub fn format_title(s: &Summary) -> String {
    let t = compact_tokens(s.total_tokens);
    format!("{t} · ${:.2}", s.cost_usd)
}

fn compact_tokens(n: i64) -> String {
    let abs = n.unsigned_abs();
    if abs >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if abs >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

/// Update the tray title from a today JSON value. Looks up the tray by id
/// "main" via the AppHandle — safe to call from any thread.
pub fn update_from_json(h: &AppHandle, v: &Value) {
    if let Some(s) = summary::from_today_json(v) {
        let title = format_title(&s);
        if let Some(tray) = h.tray_by_id("main") {
            let _ = tray.set_title(Some(&title));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_examples() {
        let s = Summary {
            period: "day".into(),
            total_tokens: 2_840_000,
            cost_usd: 4.21,
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
            messages: 0,
        };
        assert_eq!(format_title(&s), "2.84M · $4.21");
    }
}
