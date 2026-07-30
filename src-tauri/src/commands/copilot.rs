//! Copilot OAuth Device Flow login commands (two-step).
//!
//! Following the token-monitor reference pattern:
//! 1. `copilot_login` → requests device code from GitHub, returns user code + URL
//! 2. `poll_for_token` → polls until user authorizes in browser, returns access token

use tauri::AppHandle;

use crate::quota::copilot;

/// Phase 1: request device code from GitHub and return the authorize info.
/// The frontend displays the user code and opens the browser for authorization.
/// No events emitted — the authorize info is returned directly in the IPC response.
#[tauri::command]
pub async fn copilot_login() -> Result<copilot::LoginStart, String> {
    copilot::request_device_code_info()
        .await
        .map_err(|e| crate::quota::format_validate_error(&e.to_string()))
}

/// Phase 2: poll for access token. Blocks until the user completes
/// authorization in the browser. Returns the token on success.
#[tauri::command]
pub async fn poll_for_token(app: AppHandle) -> Result<String, String> {
    copilot::poll_for_access_token(&app)
        .await
        .map_err(|e| crate::quota::format_validate_error(&e.to_string()))
}
