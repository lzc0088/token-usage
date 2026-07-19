//! Config + credential commands (M3/M5).

use tauri::State;

use crate::commands::db;
use crate::config::Config;
use crate::credentials;
use crate::state::AppState;

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Result<Config, String> {
    crate::config::load(&db(&state)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_config(config: Config, state: State<AppState>) -> Result<(), String> {
    crate::config::save(&db(&state), &config).map_err(|e| e.to_string())
}

/// Check if a vendor has a stored credential in the keyring.
#[tauri::command]
pub fn get_credential_status(vendor: String) -> Result<bool, String> {
    Ok(credentials::exists(&vendor).unwrap_or(false))
}

/// Store a vendor credential.
#[tauri::command]
pub fn set_credential(vendor: String, secret: String) -> Result<(), String> {
    credentials::set(&vendor, &secret).map_err(|e| e.to_string())
}

/// Delete a vendor credential.
#[tauri::command]
pub fn delete_credential(vendor: String) -> Result<(), String> {
    credentials::delete(&vendor).map_err(|e| e.to_string())
}
