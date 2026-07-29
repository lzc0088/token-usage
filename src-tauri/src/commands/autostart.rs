//! Launch-on-boot commands (M? T5.2).
//!
//! Thin wrapper over `tauri-plugin-autostart`. The OS-level registration is the
//! source of truth (users may flip it in System Settings / LaunchAgent files);
//! [`set_auto_start`] also persists the choice into `Config` so the settings UI
//! reflects it on next launch, and [`sync_auto_start_on_boot`] reconciles the
//! OS state with the stored config at startup.

use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tracing::{debug, warn};

use crate::commands::db;
use crate::config;
use crate::state::AppState;

/// Enable / disable launch-on-boot and persist the choice in `Config`.
/// Returns the actual system state after the change.
#[tauri::command]
pub fn set_auto_start(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    let manager = app.autolaunch();

    // 1. Apply to the OS-level autostart registration.
    if enabled {
        if let Err(e) = manager.enable() {
            warn!("autostart enable failed: {e}");
            return Err(format!("启用开机启动失败: {e}"));
        }
    } else if let Err(e) = manager.disable() {
        warn!("autostart disable failed: {e}");
        return Err(format!("关闭开机启动失败: {e}"));
    }

    // 2. Persist the choice so the UI is correct on next launch.
    {
        let conn = db(&state);
        let mut cfg = config::load(&conn).unwrap_or_default();
        if cfg.auto_start != enabled {
            cfg.auto_start = enabled;
            if let Err(e) = config::save(&conn, &cfg) {
                warn!("autostart persist config failed: {e}");
            }
        }
    }

    // 3. Report the real system state (may differ from intent on failure).
    let actual = manager.is_enabled().unwrap_or(enabled);
    Ok(actual)
}

/// Read the real launch-on-boot state straight from the OS.
#[tauri::command]
pub fn get_auto_start(app: AppHandle) -> Result<bool, String> {
    Ok(app.autolaunch().is_enabled().unwrap_or(false))
}

/// Reconcile OS autostart with the stored config at app startup.
/// Called from `setup` so a wiped LaunchAgent (or a fresh install with
/// `auto_start = true` in the DB) self-heals.
pub fn sync_auto_start_on_boot(app: &AppHandle) {
    let want = {
        let state = app.state::<AppState>();
        let conn = state.db_guard();
        config::load(&conn).map(|c| c.auto_start).unwrap_or(false)
    };

    let manager = app.autolaunch();
    let current = manager.is_enabled().unwrap_or(false);
    if want && !current {
        debug!("boot-sync: enabling (config=true, os=false)");
        if let Err(e) = manager.enable() {
            warn!("boot-sync enable failed: {e}");
        }
    } else if !want && current {
        debug!("boot-sync: disabling (config=false, os=true)");
        if let Err(e) = manager.disable() {
            warn!("boot-sync disable failed: {e}");
        }
    }
}
