//! Cross-window lifecycle: open/close the standalone settings window.
//!
//! The `settings` window is pre-declared in `tauri.conf.json` (`visible:false`),
//! so these commands just show/hide it. Opening hides the main popover first
//! (mutual exclusion); closing only hides settings — the tray click (or user)
//! is responsible for re-showing main.

use tauri::{AppHandle, Manager};

/// Hide the main popover, then show + center + focus the settings window.
///
/// Window operations are deferred to avoid a deadlock: `w.show()` triggers
/// webview JS init → mount effect calls `api.getConfig()` (IPC). If the main
/// thread is still in the `invoke_handler`, the IPC blocks → webview blocks.
/// A short async delay ensures the handler has fully returned.
#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    let app_c = app.clone();
    tauri::async_runtime::spawn(async move {
        // Small delay so the invoke_handler fully returns before the webview
        // starts its JS init (which makes IPC calls back to the main thread).
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        if let Some(main) = app_c.get_webview_window("main") {
            let _ = main.hide();
        }
        if let Some(w) = app_c.get_webview_window("settings") {
            let _ = w.show();
            let _ = w.set_focus();
            tracing::info!("settings window opened");
        } else {
            tracing::warn!("settings window not found");
        }
    });
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

/// Open an external URL in the system browser. Validates that the URL uses
/// `http` or `https` scheme — rejects `javascript:`, `file:`, and other
/// dangerous schemes. This is the only way to open external links from the
/// frontend; the broad `shell:allow-open` permission is intentionally not
/// granted.
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    // Reject non-http(s) schemes at the Rust boundary.
    let scheme = url::Url::parse(&url)
        .map_err(|e| format!("invalid URL: {e}"))?
        .scheme()
        .to_string();
    if scheme != "http" && scheme != "https" {
        return Err(format!("blocked URL scheme: {scheme}"));
    }
    open::that(url).map_err(|e| format!("failed to open URL: {e}"))?;
    Ok(())
}
