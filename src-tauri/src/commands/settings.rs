//! Config commands (M3 T3.1).

use tauri::State;

use crate::commands::db;
use crate::config::Config;
use crate::state::AppState;

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Result<Config, String> {
    crate::config::load(&db(&state)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_config(config: Config, state: State<AppState>) -> Result<(), String> {
    crate::config::save(&db(&state), &config).map_err(|e| e.to_string())
}
