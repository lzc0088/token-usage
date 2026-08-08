//! Floating data widget (desktop) — M8, plan B.
//!
//! Two cooperating windows:
//! - `floating` (handle): a small persistent circular "T" button docked on
//!   screen. Hover it → the panel appears; click it → main popover; drag it →
//!   reposition (persisted). Never resizes, so it never flickers.
//! - `floating-panel` (panel): a hidden data pill, shown flush against the
//!   handle on hover so the two read as one capsule. Hidden again on
//!   hover-out.
//!
//! macOS is intentionally a no-op (the tray title already shows the readout),
//! so the widget stays hidden there regardless of config.
//!
//! Cross-window hover is bridged in Rust with a single debounced hide timer:
//! either window's mouseenter cancels it, either's mouseleave (re)starts it.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, PhysicalPosition};

use crate::config::{self, Currency};
use crate::query::range_for_period;
use crate::query::summary::{self, Summary};
use crate::ui::fmt::{compact_tokens, format_cost};
use crate::utils::time::now_ms;

/// Handle (circle) window size, physical px.
const HANDLE_W: i32 = 44;
/// Panel (data pill) window size, physical px. Height matches the handle so
/// the two form a flush capsule.
const PANEL_W: i32 = 176;
/// How far the panel slides under the handle (physical px). ~half the handle
/// lets the handle's circle mask the panel's flat inner end for a seamless join.
const OVERLAP: i32 = 22;
/// Inset from the screen edge for the default corner (logical px).
const MARGIN: f64 = 8.0;
/// kv key persisting the handle's last dragged position ("x,y" logical px).
const POS_KEY: &str = "floating_pos";
/// Hide grace period: lets the cursor cross from handle → panel without the
/// panel collapsing (the two are separate windows, so a mouseleave fires in
/// the gap).
const HIDE_GRACE_MS: i64 = 220;

/// Monotonic deadline for the pending hide. 0 = none pending. Each schedule
/// overwrites; a fired timer only hides if its deadline is still the latest.
static HIDE_DEADLINE: AtomicI64 = AtomicI64::new(0);

/// Sync widget visibility + handle position with config (startup + on change).
pub fn sync_floating(app: &AppHandle, conn: &Connection) {
    // macOS: tray title already covers this — never show the widget.
    if std::env::consts::OS == "macos" {
        hide_all(app);
        return;
    }
    let cfg = config::load(conn).unwrap_or_default();
    if cfg.floating_enabled {
        position_handle(app, conn);
        if let Some(h) = app.get_webview_window("floating") {
            let _ = h.show();
        }
        push_data(app, conn);
    } else {
        hide_all(app);
    }
}

/// Hide both windows and cancel any pending hide.
pub fn hide_all(app: &AppHandle) {
    cancel_hide();
    if let Some(h) = app.get_webview_window("floating") {
        let _ = h.hide();
    }
    if let Some(p) = app.get_webview_window("floating-panel") {
        let _ = p.hide();
    }
}

/// Position the panel against the handle and show it. The panel's inner end
/// slides ~half under the handle (overlap), and the handle is raised above
/// the panel so its circle masks the panel's flat inner end — the two then
/// read as one seamless capsule with no daylight between them. Called on
/// handle hover-in; also cancels any pending hide.
pub fn show_panel(app: &AppHandle) {
    cancel_hide();
    let (Some(h), Some(p)) = (
        app.get_webview_window("floating"),
        app.get_webview_window("floating-panel"),
    ) else {
        return;
    };
    let Ok(hp) = h.outer_position() else {
        return;
    };
    // Grow inward: if the handle is right of the monitor centre, the panel
    // extends leftward (and vice-versa) so it never runs off-screen.
    let right = is_right_side(&h);
    let px = if right {
        hp.x - PANEL_W + OVERLAP
    } else {
        hp.x + HANDLE_W - OVERLAP
    };
    let py = hp.y;
    let _ = p.set_position(PhysicalPosition::new(px, py));
    let _ = p.show();
    // Raise the handle above the panel (toggle topmost to force it to the top
    // of the always-on-top stack) so its circle cleanly masks the overlap.
    let _ = h.set_always_on_top(false);
    let _ = h.set_always_on_top(true);
}

/// (Re)start the debounced hide. When it fires (and is still the latest
/// schedule) the panel is hidden. The handle stays visible.
pub fn schedule_hide(app: &AppHandle) {
    let deadline = now_ms() + HIDE_GRACE_MS;
    HIDE_DEADLINE.store(deadline, Ordering::SeqCst);
    let app_c = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis((HIDE_GRACE_MS + 20) as u64)).await;
        if HIDE_DEADLINE.load(Ordering::SeqCst) == deadline {
            if let Some(p) = app_c.get_webview_window("floating-panel") {
                let _ = p.hide();
            }
        }
    });
}

/// Cancel any pending hide (hover is still active).
pub fn cancel_hide() {
    HIDE_DEADLINE.store(0, Ordering::SeqCst);
}

/// Push the latest readout + theme to both widget windows. The panel uses
/// text/side/theme; the handle only consumes theme (it ignores text). Emitting
/// to both keeps them in sync after a config or collector tick.
pub fn push_data(app: &AppHandle, conn: &Connection) {
    let cfg = match config::load(conn) {
        Ok(c) => c,
        Err(_) => return,
    };
    if !cfg.floating_enabled {
        return;
    }

    let rate = load_cny_rate(conn);
    let mode = cfg.floating_display.as_str();
    let range = if mode.starts_with("total_") {
        range_for_period(crate::query::Period::Total, &today_str())
    } else {
        range_for_period(crate::query::Period::Day, &today_str())
    };
    let text = match summary::query(conn, &range) {
        Ok(s) => build_text(&s, mode, cfg.currency, rate),
        Err(_) => String::new(),
    };
    let payload = FloatingData {
        text,
        side: panel_side(app),
        theme: resolved_theme(app, &cfg),
    };
    if let Some(p) = app.get_webview_window("floating-panel") {
        let _ = p.emit("floating:update", &payload);
    }
    if let Some(h) = app.get_webview_window("floating") {
        let _ = h.emit("floating:update", &payload);
    }
}

/// Resolve the effective theme ("dark" | "light") from config, following the
/// OS appearance when set to "system". Mirrors how the main/settings windows
/// pick their palette so the widget matches.
pub fn resolved_theme(app: &AppHandle, cfg: &crate::config::Config) -> String {
    match cfg.theme.as_str() {
        "dark" => "dark".into(),
        "light" => "light".into(),
        _ => match app
            .get_webview_window("floating")
            .and_then(|w| w.theme().ok())
        {
            Some(tauri::Theme::Light) => "light".into(),
            _ => "dark".into(),
        },
    }
}

/// Persist the handle's current on-screen position (called from a low-rate
/// poller, so this is cheap and always captures the final resting spot).
pub fn persist_handle_pos(app: &AppHandle) {
    let Some(h) = app.get_webview_window("floating") else {
        return;
    };
    if !h.is_visible().unwrap_or(false) {
        return;
    }
    let Ok(pos) = h.outer_position() else {
        return;
    };
    let scale = h.scale_factor().unwrap_or(1.0).max(1.0);
    let x = (pos.x as f64 / scale).round() as i32;
    let y = (pos.y as f64 / scale).round() as i32;
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = crate::config::set_raw(&conn, POS_KEY, &format!("{x},{y}"));
}

/// Restore the handle position from the kv, or fall back to the default corner.
fn position_handle(app: &AppHandle, conn: &Connection) {
    let Some(h) = app.get_webview_window("floating") else {
        return;
    };
    if let Some((x, y)) = load_pos(conn) {
        let _ = h.set_position(LogicalPosition::new(x, y));
    } else {
        place_default_corner(&h);
    }
}

/// Default resting spot: top-right of the primary monitor.
fn place_default_corner(win: &tauri::WebviewWindow) {
    let Ok(Some(mon)) = win.current_monitor() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let mw = mon.size().width as f64 / scale;
    let mx = mon.position().x as f64 / scale;
    let my = mon.position().y as f64 / scale;
    let x = mx + mw - HANDLE_W as f64 - MARGIN;
    let y = my + MARGIN;
    let _ = win.set_position(LogicalPosition::new(x, y));
}

/// Which side of the handle the panel sits on: "left" (handle is right of
/// monitor centre → panel grows leftward) or "right". Used by both the data
/// push and the initial-fetch command so the pill rounds its outer end.
pub fn panel_side(app: &AppHandle) -> String {
    match app.get_webview_window("floating") {
        Some(h) => {
            if is_right_side(&h) {
                "left".into()
            } else {
                "right".into()
            }
        }
        None => "left".into(),
    }
}

/// Is the handle right of its monitor's vertical centre? Decides which way the
/// panel grows. Defaults to true (grow leftward) when geometry is unavailable.
fn is_right_side(h: &tauri::WebviewWindow) -> bool {
    let Ok(hp) = h.outer_position() else {
        return true;
    };
    let Ok(hs) = h.outer_size() else {
        return true;
    };
    let Ok(Some(mon)) = h.current_monitor() else {
        return true;
    };
    let handle_cx = hp.x + hs.width as i32 / 2;
    let mon_cx = mon.position().x + mon.size().width as i32 / 2;
    handle_cx > mon_cx
}

/// Read the persisted handle position ("x,y" logical px), if any.
fn load_pos(conn: &Connection) -> Option<(f64, f64)> {
    let raw = crate::config::get_raw(conn, POS_KEY).ok().flatten()?;
    let mut it = raw.split(',');
    let x = it.next()?.parse::<f64>().ok()?;
    let y = it.next()?.parse::<f64>().ok()?;
    Some((x, y))
}

/// Format the display value for a resolved summary + display mode.
pub fn build_text(s: &Summary, mode: &str, currency: Currency, cny_rate: f64) -> String {
    match mode {
        "today_cost" | "total_cost" => format_cost(s.cost_usd, currency, cny_rate),
        _ => compact_tokens(s.total_tokens),
    }
}

/// Latest stored USD→CNY rate (any date). Falls back to 7.2 when unset.
pub fn load_cny_rate(conn: &Connection) -> f64 {
    conn.query_row(
        "SELECT rate FROM exchange_rate ORDER BY date DESC LIMIT 1",
        [],
        |r| r.get::<_, f64>(0),
    )
    .unwrap_or(7.2)
}

fn today_str() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Payload emitted to the panel webview.
#[derive(serde::Serialize, Clone)]
pub struct FloatingData {
    /// Pre-formatted readout, e.g. "2.8M" or "$4.2".
    pub text: String,
    /// Which side of the handle the panel sits on: "left" | "right" (rounds
    /// the pill's outer end so it reads as one capsule with the handle).
    pub side: String,
    /// Effective theme ("dark" | "light") so the widget matches the app.
    pub theme: String,
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
    fn build_text_tokens() {
        assert_eq!(
            build_text(&sample(), "today_tokens", Currency::Usd, 7.2),
            "2.8M"
        );
        assert_eq!(
            build_text(&sample(), "total_tokens", Currency::Usd, 7.2),
            "2.8M"
        );
    }

    #[test]
    fn build_text_cost() {
        assert_eq!(
            build_text(&sample(), "today_cost", Currency::Usd, 7.2),
            "$4.2"
        );
        assert_eq!(
            build_text(&sample(), "total_cost", Currency::Cny, 7.0),
            "¥29.5"
        );
    }

    #[test]
    fn build_text_unknown_defaults_to_tokens() {
        assert_eq!(build_text(&sample(), "???", Currency::Usd, 7.2), "2.8M");
    }

    #[test]
    fn load_pos_roundtrips_through_kv() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        assert!(load_pos(&conn).is_none());
        let _ = crate::config::set_raw(&conn, POS_KEY, "123.4,567.8");
        let (x, y) = load_pos(&conn).unwrap();
        assert!((x - 123.4).abs() < f64::EPSILON);
        assert!((y - 567.8).abs() < f64::EPSILON);
    }

    #[test]
    fn load_pos_ignores_malformed() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        let _ = crate::config::set_raw(&conn, POS_KEY, "garbage");
        assert!(load_pos(&conn).is_none());
    }
}
