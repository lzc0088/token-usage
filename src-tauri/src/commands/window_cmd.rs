//! Cross-window lifecycle: open/close the standalone settings window.
//!
//! The `settings` window is pre-declared in `tauri.conf.json` (`visible:false`),
//! so these commands just show/hide it. Opening hides the main popover first
//! (mutual exclusion); closing only hides settings — the tray click (or user)
//! is responsible for re-showing main.

use tauri::{AppHandle, Manager};

/// Hide the main popover, then show + center + focus the settings window.
#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    let w = app
        .get_webview_window("settings")
        .ok_or_else(|| "settings window not found".to_string())?;
    // Show the window at its last position (first launch is centred by
    // tauri.conf.json). Don't force-re-centre so the user can move it.
    let _ = w.show();
    let _ = w.set_focus();
    Ok(())
}

/// Hide the settings window only. Does not auto-show main.
#[tauri::command]
pub fn close_settings(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.hide();
    }
    Ok(())
}
