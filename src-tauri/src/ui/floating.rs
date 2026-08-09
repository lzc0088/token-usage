//! Floating data widget (desktop) — capsule fills an OPAQUE window; the window
//! resizes on hover, and its edge-side extends off-screen so the monitor clips
//! it flat (half-capsule shape) while the OS rounds the outer side.
//!
//! The window is opaque (transparent:false) → no glass/blur (a transparent
//! window rendered as glass on Windows). To still get a half-capsule (flat side
//! flush with the screen edge) without transparency, the window's edge-side is
//! pushed CLIP px beyond the screen; the monitor clips it to a straight edge,
//! and the OS (DWM) rounds the outer corners. So:
//!   - Collapsed (44 visible + CLIP off-screen): a rounded tab showing "T".
//!   - Expanded (148 visible + CLIP off-screen): a half-pill with "T" + data.
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

/// Collapsed VISIBLE width (logical px) — the on-screen semicircle.
const COLLAPSED_W: f64 = 44.0;
/// Expanded VISIBLE width (logical px) — the on-screen half-pill.
const EXPANDED_W: f64 = 148.0;
/// Window/capsule height in logical px.
const WIN_H: f64 = 44.0;
/// Off-screen clip (logical px). The window extends this far BEYOND the screen
/// edge on the "flat" side; the monitor clips it → that side renders as a
/// straight edge (so the widget looks like a half-capsule flush with the edge,
/// not a fully-rounded pill). The outer side is rounded by the OS (DWM).
const CLIP: f64 = 12.0;
/// Estimated taskbar/panel height (logical px). The widget sits above it so
/// it isn't covered by the taskbar.
const TASKBAR_H: f64 = 48.0;
/// Gap between the floating widget and the taskbar (logical px).
const FLOAT_GAP: f64 = 50.0;
/// kv key persisting the handle's on-screen position ("x,y" logical px).
const POS_KEY: &str = "floating_pos";
/// Hide grace period: lets the cursor hover the panel without it collapsing.
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
    // Only persist the collapsed resting position (COLLAPSED_W + CLIP wide).
    let collapsed = h
        .outer_size()
        .map(|s| (s.width as f64 / scale - (COLLAPSED_W + CLIP)).abs() < 1.0)
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

/// Position the floating window at its saved spot — but only if the saved edge
/// still matches the configured edge. Otherwise (first run, or the user just
/// switched edges) fall back to the default corner for the current edge.
fn position_handle(app: &AppHandle, conn: &Connection) {
    let Some(win) = app.get_webview_window("floating") else {
        return;
    };
    let cfg = config::load(conn).unwrap_or_default();
    if let Some((saved_edge, wx, wy)) = load_pos(conn) {
        if saved_edge == cfg.floating_position {
            let _ = win.set_size(LogicalSize::new(COLLAPSED_W + CLIP, WIN_H));
            let _ = win.set_position(LogicalPosition::new(wx, wy));
            return;
        }
    }
    place_default_corner(&win, &cfg.floating_position);
}

/// Default resting spot: the collapsed widget (44 visible + CLIP off-screen)
/// flush against the configured screen edge of the primary monitor, above the
/// taskbar. The edge-side extends CLIP px beyond the screen so the monitor
/// clips it flat (half-capsule look); the outer side is OS-rounded.
fn place_default_corner(win: &tauri::WebviewWindow, position: &str) {
    let Ok(Some(mon)) = win.primary_monitor() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let mw = mon.size().width as f64 / scale;
    let mh = mon.size().height as f64 / scale;
    let mx = mon.position().x as f64 / scale;
    let my = mon.position().y as f64 / scale;
    set_collapsed_geometry(
        win,
        position,
        mx,
        mw,
        my + mh - TASKBAR_H - FLOAT_GAP - WIN_H,
    );
}

/// Set the collapsed window geometry: size = COLLAPSED_W + CLIP, positioned so
/// the configured edge-side is CLIP px off-screen. `mx`/`mw` = monitor left/
/// width (logical); `top` = desired window top (logical).
fn set_collapsed_geometry(win: &tauri::WebviewWindow, position: &str, mx: f64, mw: f64, top: f64) {
    let win_w = COLLAPSED_W + CLIP;
    let _ = win.set_size(LogicalSize::new(win_w, WIN_H));
    let window_left = if position == "left" {
        mx - CLIP // left edge CLIP px off-screen
    } else {
        mx + mw - COLLAPSED_W // right edge CLIP px off-screen
    };
    let _ = win.set_position(LogicalPosition::new(window_left, top));
}

/// Grow (hover) or shrink (leave) the floating window between the collapsed
/// semicircle (44 visible) and the expanded half-pill (148 visible). The
/// edge-side stays CLIP px off-screen (so it keeps rendering as the flat side):
/// right edge → window grows leftward (right edge pinned); left edge → grows
/// rightward (left edge pinned). `expanded` = true grows, false shrinks.
pub fn resize_expanded(app: &AppHandle, expanded: bool) {
    let Some(win) = app.get_webview_window("floating") else {
        return;
    };
    let edge = app
        .try_state::<crate::state::AppState>()
        .and_then(|s| s.load_config().ok())
        .map(|c| c.floating_position)
        .unwrap_or_else(|| "right".into());
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let visible = if expanded { EXPANDED_W } else { COLLAPSED_W };
    let target_w = visible + CLIP;
    if edge == "left" {
        // Left edge pinned off-screen: just resize (grows rightward).
        let _ = win.set_size(LogicalSize::new(target_w, WIN_H));
        return;
    }
    // Right edge pinned off-screen: keep the right edge, resize, reposition.
    let Ok(pos) = win.outer_position() else {
        let _ = win.set_size(LogicalSize::new(target_w, WIN_H));
        return;
    };
    let Ok(sz) = win.outer_size() else {
        let _ = win.set_size(LogicalSize::new(target_w, WIN_H));
        return;
    };
    let right = pos.x as f64 / scale + sz.width as f64 / scale;
    let new_x = right - target_w;
    let py = pos.y as f64 / scale;
    let _ = win.set_position(LogicalPosition::new(new_x, py));
    let _ = win.set_size(LogicalSize::new(target_w, WIN_H));
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
