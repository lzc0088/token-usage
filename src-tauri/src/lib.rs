// Entry point. Backend modules grow per docs/plan.md milestones.
// M6 adds system-tray integration + auto-hide on blur.

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
use tauri::Manager;
use tray_icon::{TrayIconBuilder, TrayIconEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::open_default().expect("failed to open token-usage DB");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .setup(|app| {
            let window = app.get_webview_window("main").expect("main window");

            // ── system tray ─────────────────────────────────────────────────
            let tray_icon = {
                // tiny amber square — no file dep, cross-platform safe
                let s = 32usize;
                let mut rgba = vec![0u8; s * s * 4];
                for i in 0..s * s {
                    rgba[i * 4] = 232;
                    rgba[i * 4 + 1] = 176;
                    rgba[i * 4 + 2] = 75;
                    rgba[i * 4 + 3] = 255;
                }
                tray_icon::Icon::from_rgba(rgba, s as u32, s as u32)?
            };
            let w = window.clone();
            let tray = TrayIconBuilder::new()
                .with_id("main")
                .with_title("—")
                .with_icon(tray_icon)
                .build()?;
            TrayIconEvent::set_event_handler(Some(move |event| {
                if let TrayIconEvent::Click { .. } = event {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }));

            tray::init(tray);

            // ── auto-hide popover on blur ──────────────────────────────────
            let auto_close = app
                .state::<AppState>()
                .load_config()
                .map(|c| c.auto_close_on_blur)
                .unwrap_or(true);
            if auto_close {
                let w2 = window.clone();
                window.on_window_event(move |ev| {
                    if let tauri::WindowEvent::Focused(false) = ev {
                        let _ = w2.hide();
                    }
                });
            }

            // ── collector ──────────────────────────────────────────────────
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
