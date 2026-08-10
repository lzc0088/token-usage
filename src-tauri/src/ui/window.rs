//! Window-behaviour config application (M6). Applies the `show_in_dock`,
//! `window_display_mode`, and `hotkey` config values to the running app,
//! and re-applies them live when the config changes. Tray-title rendering
//! (`tray_display`) lives in `tray.rs`; trigger behaviour (`trigger_mode`)
//! is wired in `lib.rs`'s tray event callback.

use rusqlite::Connection;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, LogicalPosition, Manager};

use crate::config;

/// Toggle Dock icon visibility (macOS only). `show=true` → Regular policy
/// (icon in Dock); `false` → Accessory (menu-bar-only). Runtime toggling is
/// permitted by AppKit. When showing, also set the app icon image so the Dock
/// displays the real icon (dev builds run outside a .app bundle and would
/// otherwise show a generic executable icon).
#[cfg(target_os = "macos")]
pub fn apply_dock_visibility(_app: &AppHandle, show: bool) {
    use objc::{class, msg_send, sel, sel_impl};
    let cls = class!(NSApplication);
    let ns_app: *mut objc::runtime::Object = unsafe { msg_send![cls, sharedApplication] };
    // 0 = NSApplicationActivationPolicyRegular (Dock visible)
    // 1 = NSApplicationActivationPolicyAccessory (Dock hidden)
    let policy: isize = if show { 0 } else { 1 };
    let _: () = unsafe { msg_send![ns_app, setActivationPolicy: policy] };
    if show {
        set_app_icon(ns_app);
    }
}

/// Load the bundled app icon (PNG embedded at compile time) and set it as
/// `NSApp.applicationIconImage`. Fixes the generic executable icon shown in
/// the Dock for dev builds (no .app bundle).
#[cfg(target_os = "macos")]
fn set_app_icon(ns_app: *mut objc::runtime::Object) {
    use objc::{class, msg_send, sel, sel_impl};
    use std::os::raw::c_void;
    let bytes: &[u8] = include_bytes!("../../icons/icon.png");
    let nsdata: *mut objc::runtime::Object = unsafe {
        msg_send![class!(NSData), dataWithBytes: bytes.as_ptr() as *const c_void length: bytes.len()]
    };
    if nsdata.is_null() {
        return;
    }
    let alloc: *mut objc::runtime::Object = unsafe { msg_send![class!(NSImage), alloc] };
    let img: *mut objc::runtime::Object = unsafe { msg_send![alloc, initWithData: nsdata] };
    if img.is_null() {
        return;
    }
    let _: () = unsafe { msg_send![ns_app, setApplicationIconImage: img] };
}
#[cfg(not(target_os = "macos"))]
pub fn apply_dock_visibility(_app: &AppHandle, _show: bool) {}

/// Apply the main-window drag mode (macOS only). `fixed=true` locks the
/// popover position (not draggable); `false` lets the user drag it. Both
/// modes still snap under the tray on show — only draggability changes.
/// The settings window is unaffected.
#[cfg(target_os = "macos")]
pub fn apply_drag_mode(app: &AppHandle, fixed: bool) {
    let flag: i8 = if fixed { 0 } else { 1 };
    set_window_draggable(app, "main", flag != 0);
}
#[cfg(not(target_os = "macos"))]
pub fn apply_drag_mode(_app: &AppHandle, _fixed: bool) {}

/// Enable or disable native window dragging for any window by label.
/// Called from the frontend when inputs gain/lose focus, so dragging the
/// window doesn't interfere with text selection or typing.
#[cfg(target_os = "macos")]
pub fn set_window_draggable(app: &AppHandle, label: &str, enabled: bool) {
    use objc::{msg_send, sel, sel_impl};
    if let Some(w) = app.get_webview_window(label) {
        if let Ok(ns_win) = w.ns_window() {
            let win: *mut objc::runtime::Object = ns_win as *mut _;
            let flag: i8 = if enabled { 1 } else { 0 };
            let _: () = unsafe { msg_send![win, setMovableByWindowBackground: flag] };
        }
    }
}
#[cfg(not(target_os = "macos"))]
pub fn set_window_draggable(_app: &AppHandle, _label: &str, _enabled: bool) {}

/// Register (or clear) the global show/hide hotkey. Empty string unregisters
/// everything. The recorded format is e.g. `Meta+Alt+T`; modifiers are mapped
/// to the accelerator crate's vocabulary (`Meta` → `CommandOrControl`).
pub fn apply_hotkey(app: &AppHandle, hotkey: &str) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let gs = app.global_shortcut();
    // Always start clean — idempotent if nothing is registered.
    if let Err(e) = gs.unregister_all() {
        tracing::warn!("window_ctl: unregister_all failed: {e}");
    }
    if hotkey.trim().is_empty() {
        return;
    }
    let Some(accel) = map_accelerator(hotkey.trim()) else {
        tracing::warn!("window_ctl: invalid hotkey: {hotkey}");
        return;
    };
    if let Err(e) = gs.on_shortcut(accel.as_str(), |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            toggle_main(app);
        }
    }) {
        tracing::warn!("window_ctl: hotkey register failed ({accel}): {e}");
    }
}

/// Map the recorded hotkey (`Meta+Alt+T`) into an accelerator string
/// (`CommandOrControl+Alt+T`). Returns None if there is no valid main key.
/// Case-insensitive matching for modifier keys; main key must be a single
/// ASCII letter/digit, a function key (F1–F24), or a named key (Space, Tab,
/// …). Special Unicode characters (e.g. `†` from Mac Option+T) are rejected.
fn map_accelerator(stored: &str) -> Option<String> {
    if stored.is_empty() {
        return None;
    }
    let parts: Vec<&str> = stored.split('+').collect();
    let mapped: Vec<String> = parts
        .iter()
        .map(|p| {
            let lower = p.to_lowercase();
            match lower.as_str() {
                "meta" | "command" | "cmd" => "CommandOrControl".to_string(),
                "control" | "ctrl" => "Control".to_string(),
                "alt" | "option" => "Alt".to_string(),
                "shift" => "Shift".to_string(),
                _ => p.to_string(),
            }
        })
        .collect();
    let result = mapped.join("+");
    // Validate: the last segment must be a recognizable main key.
    let main = mapped.last()?;
    if !is_valid_main_key(main) {
        tracing::warn!("window_ctl: invalid main key in hotkey: {main}");
        return None;
    }
    Some(result)
}

/// A valid main key is: single ASCII letter/digit, F1–F24, or a named key
/// (Space, Tab, Escape, Enter, Insert, Delete, Home, End, PageUp, PageDown,
/// Up, Down, Left, Right, Backspace, NumpadX, etc.).
fn is_valid_main_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    // Single ASCII letter or digit.
    if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() {
        return true;
    }
    // Function keys F1–F24.
    if key.starts_with('F') && key.len() <= 3 {
        if let Ok(n) = key[1..].parse::<u8>() {
            return (1..=24).contains(&n);
        }
    }
    // Named keys.
    matches!(
        key,
        "Space"
            | "Tab"
            | "Escape"
            | "Enter"
            | "Insert"
            | "Delete"
            | "Home"
            | "End"
            | "PageUp"
            | "PageDown"
            | "Up"
            | "Down"
            | "Left"
            | "Right"
            | "Backspace"
            | "NumpadAdd"
            | "NumpadSubtract"
            | "NumpadMultiply"
            | "NumpadDivide"
            | "NumpadDecimal"
            | "NumpadEnter"
            | "Numpad0"
            | "Numpad1"
            | "Numpad2"
            | "Numpad3"
            | "Numpad4"
            | "Numpad5"
            | "Numpad6"
            | "Numpad7"
            | "Numpad8"
            | "Numpad9"
    )
}

/// Toggle main popover visibility: hide if visible, else show under the tray.
fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            crate::show_main_under_tray(app);
        }
    }
}

// ── hover trigger mode: auto-hide when the cursor leaves the tray ──────────
// Uses a generation counter so a pending hide is cancelled by a later Enter
// (re-hover) without needing to store/abort a task handle.
static HOVER_HIDE_GEN: AtomicU64 = AtomicU64::new(0);

/// Cancel any pending hover-hide (call on tray Enter).
pub fn cancel_hover_hide() {
    HOVER_HIDE_GEN.fetch_add(1, Ordering::SeqCst);
}

/// Schedule the popover to hide shortly after the cursor leaves the tray.
/// The hide is skipped if the window gained focus (user moved into it — the
/// existing blur handler will dismiss it instead), and is invalidated by a
/// subsequent Enter.
pub fn schedule_hover_hide(app: AppHandle) {
    let gen = HOVER_HIDE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if HOVER_HIDE_GEN.load(Ordering::SeqCst) != gen {
            return; // superseded by a newer Enter/Leave
        }
        if let Some(w) = app.get_webview_window("main") {
            // Skip if the user moved into the window (focused) — blur handles it.
            if !w.is_focused().unwrap_or(false) {
                let _ = w.hide();
            }
        }
    });
}

/// Apply the window-behaviour settings that are safe to set after the app is
/// fully launched (drag mode, hotkey, tray title). Dock activation policy is
/// NOT included here — it must be set before window creation (see
/// `apply_dock_visibility`), so it is applied separately early in setup.
/// Place the main popover anchored at the bottom-right corner of the primary
/// monitor: right edge inset 30 px, bottom edge inset 16 px (taskbar gap).
pub fn position_main_at_edge(app: &AppHandle, _edge: &str) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let Ok(Some(mon)) = win.primary_monitor() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let mw = mon.size().width as f64 / scale;
    let mh = mon.size().height as f64 / scale;
    let mx = mon.position().x as f64 / scale;
    let my = mon.position().y as f64 / scale;
    let Ok(size) = win.outer_size() else {
        return;
    };
    let ww = size.width as f64 / scale;
    let wh = size.height as f64 / scale;
    const RIGHT_MARGIN: f64 = 30.0;
    const BOTTOM_MARGIN: f64 = 50.0;
    let x = mx + mw - ww - RIGHT_MARGIN;
    let y = my + mh - wh - BOTTOM_MARGIN;
    let _ = win.set_position(LogicalPosition::new(x, y));
}

pub fn apply_window_features(app: &AppHandle, conn: &Connection) {
    let cfg = match config::load(conn) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("window_ctl: config load failed: {e}");
            return;
        }
    };
    apply_drag_mode(app, cfg.window_display_mode == "fixed");
    apply_hotkey(app, &cfg.hotkey);
    // Repaint tray title immediately (tray_display) instead of waiting for
    // the next collector tick.
    crate::ui::tray::refresh_from_db(app, conn);
}

/// Apply ALL window-behaviour settings from the persisted config. Used by the
/// `config:changed` event listener for live updates (the app is already
/// running by then, so re-setting the dock policy is safe).
pub fn apply_window_config(app: &AppHandle, conn: &Connection) {
    let cfg = match config::load(conn) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("window_ctl: config load failed: {e}");
            return;
        }
    };
    apply_dock_visibility(app, cfg.show_in_dock);
    apply_drag_mode(app, cfg.window_display_mode == "fixed");
    apply_hotkey(app, &cfg.hotkey);
    // Main popover always-on-top — floating mode keeps it above other apps.
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_always_on_top(cfg.window_display_mode == "always_on_top");
    }
    crate::ui::tray::refresh_from_db(app, conn);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_meta_to_command_or_control() {
        assert_eq!(
            map_accelerator("Meta+Alt+T").unwrap(),
            "CommandOrControl+Alt+T"
        );
    }

    #[test]
    fn maps_control_and_shift() {
        assert_eq!(
            map_accelerator("Control+Shift+K").unwrap(),
            "Control+Shift+K"
        );
    }

    #[test]
    fn empty_returns_none() {
        assert!(map_accelerator("").is_none());
    }

    #[test]
    fn rejects_unicode_main_key() {
        // † (Mac Option+T) is not a valid Tauri accelerator key.
        assert!(map_accelerator("Meta+Alt+†").is_none());
    }

    #[test]
    fn accepts_function_keys() {
        assert_eq!(map_accelerator("Meta+F12").unwrap(), "CommandOrControl+F12");
    }
}
