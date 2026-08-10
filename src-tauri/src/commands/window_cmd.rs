//! Cross-window lifecycle: open/close the standalone settings window.
//!
//! The `settings` window is pre-declared in `tauri.conf.json` (`visible:false`),
//! so these commands just show/hide it. Opening hides the main popover first
//! (mutual exclusion); closing only hides settings — the tray click (or user)
//! is responsible for re-showing main.
//!
//! The main popover and the settings window are SEPARATE webviews with
//! independent JS contexts, so the target page (e.g. "account" from a quota
//! empty-state link) is bridged through `AppState.settings_target` (a
//! `Mutex<Option<String>>`). `open_settings` sets it; the settings window's
//! focus handler consumes it via `consume_settings_target`. `None` after a
//! take means the focus came from app-switching (not an open), so the window
//! leaves the user's current page alone.

use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Hide the main popover, show + focus the settings window, and record the
/// target page the settings window should navigate to on focus.
///
/// `target`: `None` (or omitted) → land on "general" (default). A specific id
/// like "account" → land on that page (used by quota empty-state quick links).
#[tauri::command]
pub fn open_settings(
    target: Option<String>,
    app: AppHandle,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    // Record the landing page BEFORE showing — the focus handler consumes it.
    // Empty/whitespace or omitted → "general".
    let page = target
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "general".to_string());
    if let Ok(mut g) = state.settings_target.lock() {
        *g = Some(page.clone());
    }
    let app_c = app.clone();
    tauri::async_runtime::spawn(async move {
        // Small delay so the invoke_handler fully returns before the webview
        // starts its JS init (which makes IPC calls back to the main thread).
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        show_settings_window(&app_c);
        tracing::info!("settings window opened → landing page: {page}");
    });
    Ok(())
}

/// Show + focus the settings window, hiding the main popover first. Shared by
/// `open_settings` and the tray menu's "settings" item.
pub fn show_settings_window(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        // The settings window uses Tauri's `window.startDragging()` from a
        // designated drag handle (left nav) rather than MovableByWindowBackground,
        // which conflicts with row-drag on macOS (the OS intercepts mousedown
        // at the NSWindow level before CSS `app-region` can block it).
    } else {
        tracing::warn!("settings window not found");
    }
}

/// Record the landing page for the next settings-window focus (used by the
/// tray menu, which doesn't go through the `open_settings` command).
pub fn set_settings_target(state: &AppState, page: &str) {
    if let Ok(mut g) = state.settings_target.lock() {
        *g = Some(page.to_string());
    }
}

/// Consume the pending landing page. Returns `Some(page)` if an open is
/// pending (the settings window should navigate), `None` if there is none
/// (focus came from app-switching — leave the current page alone).
#[tauri::command]
pub fn consume_settings_target(state: tauri::State<AppState>) -> Result<Option<String>, String> {
    let taken = if let Ok(mut g) = state.settings_target.lock() {
        g.take()
    } else {
        None
    };
    Ok(taken)
}

/// Bridge frontend console logs to the Rust terminal (dev diagnostics).
/// The webview's own devtools Console isn't always accessible, so the frontend
/// can call this to surface a log line in `npm run tauri dev`'s terminal.
#[tauri::command]
pub fn frontend_log(msg: String) {
    // Dedicated `frontend` target so the file-log layer can exclude it:
    // frontend diagnostics may carry OAuth codes / credential field names that
    // must NOT be persisted to disk. They still print to stdout in dev.
    tracing::info!(target: "frontend", "[FE] {msg}");
}
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

/// Mark the main window as being dragged/resized (frontend sets this on
/// drag-region pointerdown). While true, the blur-hide is suppressed so the
/// window isn't yanked away mid-interaction. Cleared on refocus (Rust).
#[tauri::command]
pub fn set_main_interacting(interacting: bool) {
    crate::MAIN_INTERACTING.store(interacting, std::sync::atomic::Ordering::SeqCst);
}

/// Temporarily enable/disable native window dragging. Used by the frontend
/// when input fields gain/lose focus — dragging is disabled while the user
/// is typing or selecting text, then restored when the input loses focus.
#[tauri::command]
pub fn set_window_draggable(label: String, enabled: bool, app: AppHandle) -> Result<(), String> {
    crate::ui::window::set_window_draggable(&app, &label, enabled);
    Ok(())
}

/// Temporarily disable native window dragging (e.g. while a row-drag is in
/// progress) and restore the per-window baseline on resume.
///
/// `suspended = true`  → `MovableByWindowBackground = false`; label tracked.
/// `suspended = false` → removed from the set; baseline restored:
///   - "settings"  → always draggable (the settings window is opened with
///                    native drag enabled in `show_settings_window`).
///   - "main"      → `cfg.window_display_mode != "fixed"` (user setting).
///   - any other   → drag enabled (safe default).
///
/// The suspended set ensures `false` is idempotent and that nested suspend /
/// resume pairs (e.g. multiple drag systems on the same window) don't
/// accidentally re-enable drag while another caller still wants it off.
#[tauri::command]
pub fn set_drag_suspended(
    label: String,
    suspended: bool,
    state: tauri::State<AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let mut set = state.drag_suspended.lock().map_err(|e| e.to_string())?;
    let was_in = set.contains(&label);
    if suspended {
        if !was_in {
            set.insert(label.clone());
            crate::ui::window::set_window_draggable(&app, &label, false);
        }
        return Ok(());
    }
    if !was_in {
        return Ok(()); // resume with no prior suspend → no-op
    }
    set.remove(&label);
    drop(set);
    let main_baseline = state
        .load_config()
        .map(|c| c.window_display_mode != "fixed")
        .unwrap_or(true);
    let baseline = drag_baseline(&label, main_baseline);
    crate::ui::window::set_window_draggable(&app, &label, baseline);
    Ok(())
}

fn drag_baseline(label: &str, main_baseline: bool) -> bool {
    match label {
        // Settings uses Tauri's `window.startDragging()` from a designated
        // handle (left nav) — no MovableByWindowBackground needed.
        "settings" => false,
        "main" => main_baseline,
        _ => true,
    }
}

// ── Floating widget commands (single window, CSS slide) ────────────────────

/// Toggle the main popover's visibility. Called when the floating widget is
/// clicked: open it if closed, close it if open. The floating widget itself
/// stays visible (so the user can click it again to dismiss the popover).
/// When opening, the popover is placed at the configured screen edge.
#[tauri::command]
pub fn toggle_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let edge = app
                .state::<crate::state::AppState>()
                .load_config()
                .map(|c| c.floating_position)
                .unwrap_or_else(|_| "right".into());
            crate::ui::window::position_main_at_edge(&app, &edge);
            let _ = w.show();
            // Ensure main appears above the floating widget (which is always-on-top
            // in tauri.conf.json). Without this, the floating widget can occlude
            // the main window on Windows where Z-order follows creation order.
            let _ = w.set_always_on_top(true);
            let _ = w.set_focus();
        }
    }
    Ok(())
}

/// Show the floating window (hover-in cancels any pending hide).
#[tauri::command]
pub fn show_floating_panel(app: AppHandle) -> Result<(), String> {
    crate::ui::floating::show_panel(&app);
    if let Some(w) = app.get_webview_window("floating") {
        let _ = w.show();
    }
    Ok(())
}

/// (Re)start the debounced hide (hover-out).
#[tauri::command]
pub fn hide_floating_panel(app: AppHandle) -> Result<(), String> {
    crate::ui::floating::schedule_hide(&app);
    Ok(())
}

/// Grow the floating window to the expanded half-pill (hover-in). Keeps the
/// screen-edge side fixed. Eliminates the large transparent area that Windows
/// WebView2 renders as glass.
#[tauri::command]
pub fn expand_floating(app: AppHandle) -> Result<(), String> {
    crate::ui::floating::resize_expanded(&app, true);
    Ok(())
}

/// Shrink the floating window back to the collapsed semicircle (hover-out).
#[tauri::command]
pub fn collapse_floating(app: AppHandle) -> Result<(), String> {
    crate::ui::floating::resize_expanded(&app, false);
    Ok(())
}

/// Return the floating widget's readout + theme for its initial paint.
#[tauri::command]
pub fn get_floating_data(app: AppHandle) -> Result<crate::ui::floating::FloatingData, String> {
    let state = app.state::<crate::state::AppState>();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let cfg = crate::config::load(&conn).map_err(|e| e.to_string())?;
    let cny_rate = crate::ui::floating::load_cny_rate(&conn);

    let mode = cfg.floating_display.as_str();
    let range = if mode.starts_with("total_") {
        crate::query::range_for_period(
            crate::query::Period::Total,
            &chrono::Local::now().format("%Y-%m-%d").to_string(),
        )
    } else {
        crate::query::range_for_period(
            crate::query::Period::Day,
            &chrono::Local::now().format("%Y-%m-%d").to_string(),
        )
    };

    let text = match crate::query::summary::query(&conn, &range) {
        Ok(s) => crate::ui::floating::build_text(&s, mode, cfg.currency, cny_rate),
        Err(_) => String::new(),
    };
    Ok(crate::ui::floating::FloatingData {
        text,
        theme: crate::ui::floating::resolved_theme(&app, &cfg),
        position: cfg.floating_position.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_baseline_is_always_off_for_settings() {
        // Settings uses Tauri startDragging(), not MovableByWindowBackground.
        assert!(!drag_baseline("settings", true));
        assert!(!drag_baseline("settings", false));
    }

    #[test]
    fn drag_baseline_tracks_main_baseline() {
        assert!(drag_baseline("main", true));
        assert!(!drag_baseline("main", false));
    }

    #[test]
    fn drag_baseline_defaults_on_for_unknown_labels() {
        assert!(drag_baseline("anything-else", true));
        assert!(drag_baseline("anything-else", false));
    }
}
