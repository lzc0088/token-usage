//! Tool-collection status (M3 T3.1). Async — spawns `tokscale clients`.

use serde::Serialize;

use crate::collector::tokscale;
use crate::paths;

/// One tool's tracking status, for the 采集 segment / hero tool dots.
#[derive(Debug, Clone, Serialize)]
pub struct ClientStatus {
    pub client: String,
    pub label: String,
    /// `active` (installed + has data) | `waiting` (installed, no data yet) | `missing`.
    pub status: &'static str,
    pub message_count: i64,
}

#[tauri::command]
pub async fn get_tools_status() -> Result<Vec<ClientStatus>, String> {
    let data = tokscale::app_bin_dir().ok_or("no platform data dir")?;
    let bin = tokscale::resolve_bin(None, &data).map_err(|e| e.to_string())?;
    let report = paths::fetch_clients(&bin)
        .await
        .map_err(|e| e.to_string())?;
    Ok(report
        .clients
        .into_iter()
        .map(|c| {
            let status = if !c.sessions_path_exists {
                "missing"
            } else if c.message_count == 0 {
                "waiting"
            } else {
                "active"
            };
            ClientStatus {
                client: c.client,
                label: c.label,
                status,
                message_count: c.message_count,
            }
        })
        .collect())
}
