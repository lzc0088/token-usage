//! Floating data widget (desktop) — capsule fills an OPAQUE window with SQUARE
//! corners, flush against the configured screen edge; the window resizes on
//! hover.
//!
//! The window is opaque (transparent:false) → no glass/blur. On Windows 11 the
//! DWM default corner rounding is disabled (DWMWCP_DONOTROUND) so the widget
//! has crisp 直角 corners; with square corners a flush position has no gap, so
//! no off-screen clipping trick is needed. The window is re-anchored to the
//! screen edge on every resize (not derived from its current position) so
//! repeated hover/leave can't drift it.
//!   - Collapsed (44 wide): a square-cornered tab showing "T".
//!   - Expanded (148 wide): a square-cornered pill with "T" + data.
//!
//! macOS is intentionally a no-op (the tray title already shows the readout),
//! so the widget stays hidden there regardless of config.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};

use crate::config::{self, Currency};
use crate::query::range_for_period;
use crate::query::summary::{self, Summary};
use crate::ui::fmt::{compact_tokens, format_cost};
use crate::utils::time::now_ms;

/// Collapsed window width (logical px) — the on-screen tab.
const COLLAPSED_W: f64 = 44.0;
/// Expanded window width (logical px) — the on-screen pill.
const EXPANDED_W: f64 = 148.0;
/// Window/capsule height in logical px.
const WIN_H: f64 = 44.0;
/// kv key persisting the handle's on-screen position ("x,y" logical px).
const POS_KEY: &str = "floating_pos";
/// Hide grace period: lets the cursor hover the panel without it collapsing.
const HIDE_GRACE_MS: i64 = 220;

/// Monotonic deadline for the pending hide. 0 = none pending. Each schedule
/// overwrites; a fired timer only hides if its deadline is still the latest.
static HIDE_DEADLINE: AtomicI64 = AtomicI64::new(0);

/// Force SQUARE window corners on Windows 11 (DWMWCP_DONOTROUND) so the widget
/// has crisp 直角 corners instead of the system's default ~8px rounding. No-op
/// elsewhere (macOS is hidden; Linux has no such default).
#[cfg(windows)]
fn ensure_square_corners(win: &tauri::WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
    // DWMWA_WINDOW_CORNER_PREFERENCE = 33; DWMWCP_DONOTROUND = 1.
    const DWMWCP_DONOTROUND: i32 = 1;
    if let Ok(hwnd) = win.hwnd() {
        let pref = DWMWCP_DONOTROUND;
        // Safety: standard DWM attribute call with a valid HWND + i32 value.
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(33),
                &pref as *const i32 as *const _,
                std::mem::size_of::<i32>() as u32,
            );
        }
    }
}
#[cfg(not(windows))]
fn ensure_square_corners(_win: &tauri::WebviewWindow) {}

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
            ensure_square_corners(&h);
            let _ = h.show();
        }
        push_data(app, conn);
    } else {
        hide_all(app);
    }
}

/// Hide the window and cancel any pending hide.
pub fn hide_all(app: &AppHandle) {
    cancel_hide();
    if let Some(h) = app.get_webview_window("floating") {
        let _ = h.hide();
    }
}

/// Show the floating window (hover-in cancels any pending hide).
pub fn show_panel(_app: &AppHandle) {
    cancel_hide();
    // The window may have been hidden by a previous schedule_hide; show it
    // so the hover CSS transition can play. Visibility is toggled by
    // mouseenter/mouseleave → show_floating_panel / hide_floating_panel.
}

/// (Re)start the debounced hide. When it fires (and is still the latest
/// schedule) the window is hidden. The CSS hover state keeps the panel
/// expanded while the cursor is inside the window.
pub fn schedule_hide(app: &AppHandle) {
    let deadline = now_ms() + HIDE_GRACE_MS;
    HIDE_DEADLINE.store(deadline, Ordering::SeqCst);
    let app_c = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis((HIDE_GRACE_MS + 20) as u64)).await;
        if HIDE_DEADLINE.load(Ordering::SeqCst) == deadline {
            if let Some(w) = app_c.get_webview_window("floating") {
                let _ = w.hide();
            }
        }
    });
}

/// Cancel any pending hide (hover is still active).
pub fn cancel_hide() {
    HIDE_DEADLINE.store(0, Ordering::SeqCst);
}

/// Push the latest readout + theme to the floating window.
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
        theme: resolved_theme(app, &cfg),
        position: cfg.floating_position.clone(),
    };
    if let Some(w) = app.get_webview_window("floating") {
        let _ = w.emit("floating:update", &payload);
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

/// Persist the floating window's resting (collapsed) position, called from a
/// low-rate poller. Skipped while expanded so we never save the transient
/// expanded geometry (which would misalign the widget after a restore).
pub fn persist_handle_pos(app: &AppHandle) {
    let Some(h) = app.get_webview_window("floating") else {
        return;
    };
    if !h.is_visible().unwrap_or(false) {
        return;
    }
    let scale = h.scale_factor().unwrap_or(1.0).max(1.0);
    // Only persist the collapsed resting position (COLLAPSED_W wide).
    let collapsed = h
        .outer_size()
        .map(|s| (s.width as f64 / scale - COLLAPSED_W).abs() < 1.0)
        .unwrap_or(true);
    if !collapsed {
        return;
    }
    let Ok(pos) = h.outer_position() else {
        return;
    };
    let wx = (pos.x as f64 / scale).round() as i32;
    let wy = (pos.y as f64 / scale).round() as i32;
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return,
    };
    // Store the configured edge alongside the position so a later edge change
    // invalidates the saved spot (the widget re-defaults to the new edge).
    let edge = config::load(&conn)
        .map(|c| c.floating_position)
        .unwrap_or_else(|_| "right".into());
    let _ = crate::config::set_raw(&conn, POS_KEY, &format!("{edge},{wx},{wy}"));
}

/// Compute the collapsed-widget top coordinate: 30 % from the monitor top,
/// centred vertically around that line.
fn default_top(my: f64, mh: f64) -> f64 {
    (my + mh * 0.30).max(my) - WIN_H / 2.0
}

/// Position the floating window at its saved spot — but only if the saved edge
/// still matches the configured edge. The Y is always recalculated (30 % from
/// top) so a prior bottom-anchor doesn't persist across code changes.
fn position_handle(app: &AppHandle, conn: &Connection) {
    let Some(win) = app.get_webview_window("floating") else {
        return;
    };
    let cfg = config::load(conn).unwrap_or_default();
    if let Some((saved_edge, _wx, _wy)) = load_pos(conn) {
        if saved_edge == cfg.floating_position {
            let Ok(Some(mon)) = win.primary_monitor() else {
                return;
            };
            let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
            let mw = mon.size().width as f64 / scale;
            let mh = mon.size().height as f64 / scale;
            let mx = mon.position().x as f64 / scale;
            let my = mon.position().y as f64 / scale;
            let window_left = if saved_edge == "left" {
                mx
            } else {
                mx + mw - COLLAPSED_W
            };
            let _ = win.set_size(LogicalSize::new(COLLAPSED_W, WIN_H));
            let _ = win.set_position(LogicalPosition::new(window_left, default_top(my, mh)));
            return;
        }
    }
    place_default_corner(&win, &cfg.floating_position);
}

/// Default resting spot: the collapsed widget (44 wide) flush against the
/// configured screen edge of the primary monitor, at ~30 % from the top.
fn place_default_corner(win: &tauri::WebviewWindow, position: &str) {
    let Ok(Some(mon)) = win.primary_monitor() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let mw = mon.size().width as f64 / scale;
    let mh = mon.size().height as f64 / scale;
    let mx = mon.position().x as f64 / scale;
    let my = mon.position().y as f64 / scale;
    let top = default_top(my, mh);
    set_collapsed_geometry(win, position, mx, mw, top);
}

/// Set the collapsed window geometry: size = COLLAPSED_W, flush against the
/// configured screen edge. `mx`/`mw` = monitor left/width (logical); `top` =
/// desired window top (logical). Corners are square (DWM rounding disabled on
/// Windows — see ensure_square_corners), so a flush position has no gap.
fn set_collapsed_geometry(win: &tauri::WebviewWindow, position: &str, mx: f64, mw: f64, top: f64) {
    let _ = win.set_size(LogicalSize::new(COLLAPSED_W, WIN_H));
    let window_left = if position == "left" {
        mx
    } else {
        mx + mw - COLLAPSED_W
    };
    let _ = win.set_position(LogicalPosition::new(window_left, top));
}

/// Grow (hover) or shrink (leave) the floating window between the collapsed tab
/// (44) and the expanded pill (148). The window is re-anchored to the SCREEN
/// EDGE every call (not derived from its current position), so repeated
/// hover/leave can't drift it toward the edge. Only the Y is preserved.
pub fn resize_expanded(app: &AppHandle, expanded: bool) {
    let Some(win) = app.get_webview_window("floating") else {
        return;
    };
    let edge = app
        .try_state::<crate::state::AppState>()
        .and_then(|s| s.load_config().ok())
        .map(|c| c.floating_position)
        .unwrap_or_else(|| "right".into());
    let Ok(Some(mon)) = win.primary_monitor() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let mw = mon.size().width as f64 / scale;
    let mx = mon.position().x as f64 / scale;
    let visible = if expanded { EXPANDED_W } else { COLLAPSED_W };
    // Preserve the current Y (vertical drag); recompute X from the screen edge.
    let py = win
        .outer_position()
        .map(|p| p.y as f64 / scale)
        .unwrap_or_else(|_| mon.position().y as f64 / scale);
    let new_x = if edge == "left" {
        mx // left edge flush, grows rightward
    } else {
        mx + mw - visible // right edge flush, grows leftward
    };
    let _ = win.set_position(LogicalPosition::new(new_x, py));
    let _ = win.set_size(LogicalSize::new(visible, WIN_H));
}

/// Read the persisted window position ("edge,x,y" logical px), if any.
fn load_pos(conn: &Connection) -> Option<(String, f64, f64)> {
    let raw = crate::config::get_raw(conn, POS_KEY).ok().flatten()?;
    let mut it = raw.split(',');
    let edge = it.next()?.to_string();
    let x = it.next()?.parse::<f64>().ok()?;
    let y = it.next()?.parse::<f64>().ok()?;
    Some((edge, x, y))
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

/// Payload emitted to the floating webview.
#[derive(serde::Serialize, Clone)]
pub struct FloatingData {
    /// Pre-formatted readout, e.g. "2.8M" or "$4.2".
    pub text: String,
    /// Effective theme ("dark" | "light") so the widget matches the app.
    pub theme: String,
    /// Screen edge ("left" | "right") — drives the capsule's CSS variant.
    pub position: String,
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
        let _ = crate::config::set_raw(&conn, POS_KEY, "right,123,568");
        let (edge, x, y) = load_pos(&conn).unwrap();
        assert_eq!(edge, "right");
        assert!((x - 123.0).abs() < f64::EPSILON);
        assert!((y - 568.0).abs() < f64::EPSILON);
    }

    #[test]
    fn load_pos_ignores_malformed() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        let _ = crate::config::set_raw(&conn, POS_KEY, "garbage");
        assert!(load_pos(&conn).is_none());
        // Edge but missing coordinates.
        let _ = crate::config::set_raw(&conn, POS_KEY, "right");
        assert!(load_pos(&conn).is_none());
    }

    #[test]
    fn no_saved_position_yields_none() {
        // No persisted spot → load_pos returns None → caller uses the default
        // corner for the configured edge.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        let pos = load_pos(&conn);
        assert!(
            pos.is_none(),
            "no saved position → should use default corner"
        );
    }
}
