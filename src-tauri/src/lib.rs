// Entry point. Backend modules grow per docs/plan.md milestones.
// M3 wires commands + managed DB state into the Tauri builder.

pub mod collector;
pub mod commands;
pub mod config;
pub mod credentials;
pub mod paths;
pub mod query;
pub mod quota;
pub mod state;
pub mod storage;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::open_default().expect("failed to open token-usage DB");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::query::get_summary,
            commands::query::get_breakdown,
            commands::query::get_trends,
            commands::query::get_sessions,
            commands::query::get_projects,
            commands::status::get_tools_status,
            commands::settings::get_config,
            commands::settings::set_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
