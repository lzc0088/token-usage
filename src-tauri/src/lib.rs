// Entry point. M6: Tauri-native system tray + auto-hide on blur.

pub mod collector;
pub mod commands;
pub mod config;
pub mod credentials;
pub mod paths;
pub mod query;
pub mod quota;
pub mod state;
pub mod storage;
pub mod tray;

use state::AppState;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::open_default().expect("failed to open token-usage DB");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .setup(|app| {
            let window = app.get_webview_window("main").expect("main window");

            // ── system tray (Tauri-native, integrated with the event loop) ──
            let icon = app
                .default_window_icon()
                .cloned()
                .expect("no default window icon");
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip("Token Usage")
                .on_tray_icon_event(|tray, event| {
                    // Left-click toggles popover visibility.
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            // ── auto-hide popover on blur ──────────────────────────────────
            let auto_close = app
                .state::<AppState>()
                .load_config()
                .map(|c| c.auto_close_on_blur)
                .unwrap_or(true);
            if auto_close {
                let w = window.clone();
                window.on_window_event(move |ev| {
                    if let tauri::WindowEvent::Focused(false) = ev {
                        let _ = w.hide();
                    }
                });
            }

            // ── collector (watcher + scheduler + consumer) ──────────────────
            let h = app.handle().clone();
            let db = app.state::<AppState>().db.clone();
            tauri::async_runtime::spawn(collector::runtime::start(h, db));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::query::get_summary,
            commands::query::get_breakdown,
            commands::query::get_trends,
            commands::query::get_sessions,
            commands::query::get_projects,
            commands::status::get_tools_status,
            commands::status::get_tokscale_status,
            commands::quota::get_quotas,
            commands::settings::get_config,
            commands::settings::set_config,
            commands::settings::get_credential_status,
            commands::settings::set_credential,
            commands::settings::delete_credential,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
