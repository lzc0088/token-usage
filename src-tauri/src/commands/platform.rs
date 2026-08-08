//! Platform detection command (M7). Returns the current OS as a lowercase
//! string: "macos", "windows", or "linux".

/// Return the current operating system as a string.
/// Values: "macos", "windows", "linux".
#[tauri::command]
pub fn get_platform() -> &'static str {
    std::env::consts::OS
}
