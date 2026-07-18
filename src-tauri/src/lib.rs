// T0.2 scaffold entry point. Real backend modules (collector/storage/query/
// commands) are added in M1–M2 per docs/plan.md.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
