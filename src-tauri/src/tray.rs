//! System tray helper (M6). `TrayIcon` is not `Send` (contains `Rc<…>`), so we
//! store it in a `thread_local!` on the main thread. Title updates from the
//! collector consumer are dispatched via `AppHandle::run_on_main_thread`.

use std::cell::RefCell;

use serde_json::Value;
use tauri::AppHandle;
use tray_icon::TrayIcon;

use crate::query::summary::{self, Summary};

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

/// Store the tray icon (call once from setup on the main thread).
pub fn init(tray: TrayIcon) {
    TRAY.with(|t| *t.borrow_mut() = Some(tray));
}

/// Dispatch a title update to the main thread. Safe to call from any thread.
pub fn set_title(h: &AppHandle, title: String) {
    let _ = h.run_on_main_thread(move || {
        TRAY.with(|t| {
            if let Some(ref tray) = *t.borrow() {
                tray.set_title(Some(title.as_str()));
            }
        });
    });
}

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

/// Update the tray title from a today JSON value (called from the consumer).
pub fn update_from_json(h: &AppHandle, v: &Value) {
    if let Some(s) = summary::from_today_json(v) {
        set_title(h, format_title(&s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_examples() {
        let s = Summary {
            period: "day".into(),
            total_tokens: 0,
            cost_usd: 0.0,
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
            messages: 0,
        };
        assert_eq!(format_title(&s), "0 · $0.00");
    }
}
