//! Desktop widget window — the `widget` webview pinned to the desktop
//! (macOS: desktop-icon window level, stationary across Spaces; other
//! platforms: a normal small window). Shows today's usage, the tightest
//! quotas, and a 7-day sparkline; the page itself lives in
//! `src/WidgetApp.svelte` (routed by window label, same SPA as settings).
//!
//! Visibility and last position persist via config, mirroring the floating
//! widget's sync/persist pattern.

use rusqlite::Connection;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::config;

/// Show/hide the widget per `config.widget_enabled` and restore its last
/// position. Safe to call repeatedly (config changes, launch).
pub fn sync_widget(app: &AppHandle, conn: &Connection) {
    let Some(win) = app.get_webview_window("widget") else {
        return;
    };
    apply_desktop_level(&win);
    let cfg = config::load(conn).unwrap_or_default();
    if cfg.widget_enabled {
        if let Some((x, y)) = cfg.widget_pos {
            let _ = win.set_position(tauri::LogicalPosition::new(x, y));
        }
        let _ = win.show();
    } else {
        let _ = win.hide();
    }
}

/// Pin the window to the desktop layer (macOS only). Level
/// `kCGDesktopIconWindowLevel` (-2147483603) sits above the wallpaper and
/// below every normal window; the collection behavior keeps the card
/// stationary across Spaces and out of the Command-Tab cycle.
#[cfg(target_os = "macos")]
fn apply_desktop_level(win: &WebviewWindow) {
    use objc::{msg_send, sel, sel_impl};
    if let Ok(ns_win) = win.ns_window() {
        let w: *mut objc::runtime::Object = ns_win as *mut _;
        unsafe {
            // kCGDesktopIconWindowLevel = CGWindowLevelForKey(kCGDesktopIconWindow)
            let _: () = msg_send![w, setLevel: -2147483603i64];
            // NSWindowCollectionBehaviorCanJoinAllSpaces(1<<1)
            // | Stationary(1<<5) | IgnoresCycle(1<<8)
            let _: () =
                msg_send![w, setCollectionBehavior: (1u64 << 1) | (1u64 << 5) | (1u64 << 8)];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_desktop_level(_win: &WebviewWindow) {}

/// Persist the widget's resting position (low-rate poll, same pattern as
/// `floating::persist_handle_pos` — captures the position after a drag
/// without per-move bookkeeping). No-op while hidden.
pub fn persist_widget_pos(app: &AppHandle) {
    let Some(win) = app.get_webview_window("widget") else {
        return;
    };
    if !win.is_visible().unwrap_or(false) {
        return;
    }
    let Ok(pos) = win.outer_position() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let logical = (
        (pos.x as f64 / scale).round(),
        (pos.y as f64 / scale).round(),
    );
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let Ok(conn) = state.db.lock() else {
        return;
    };
    let mut cfg = config::load(&conn).unwrap_or_default();
    if cfg.widget_pos != Some(logical) {
        cfg.widget_pos = Some(logical);
        if let Err(e) = config::save(&conn, &cfg) {
            tracing::warn!(error = %e, "widget position persist failed");
        }
        state.update_config_cache(cfg);
    }
}
