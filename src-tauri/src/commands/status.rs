//! Tool-collection status (M3 T3.1). Async — spawns `tokscale clients`.

use serde::Serialize;

use crate::collector::tokscale;
use crate::install_probe;
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
pub async fn get_tools_status(app: tauri::AppHandle) -> Result<Vec<ClientStatus>, String> {
    let data = tokscale::app_bin_dir().ok_or("no platform data dir")?;
    let custom = tokscale::bundled_bin_path(&app);
    let bin = tokscale::resolve_bin(custom.as_deref(), &data).map_err(|e| e.to_string())?;
    let report = paths::fetch_clients(&bin)
        .await
        .map_err(|e| e.to_string())?;
    Ok(report
        .clients
        .into_iter()
        .map(|c| {
            // "Installed" = the tool is on the machine: either it has session
            // data, tokscale's known sessions dir exists, or a curated install
            // probe (macOS .app bundle / config dir) matches. This fixes tools
            // like Warp (installed GUI app, no sessions yet) and zcode (data
            // under a non-standard path) that previously showed as 未安装.
            let installed = c.sessions_path_exists
                || c.message_count > 0
                || install_probe::is_installed(&c.client);
            let status = if c.message_count > 0 {
                "active"
            } else if installed {
                "waiting"
            } else {
                "missing"
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct TokscaleStatus {
    pub installed: bool,
    pub version: Option<String>,
}

#[tauri::command]
pub async fn get_tokscale_status(app: tauri::AppHandle) -> Result<TokscaleStatus, String> {
    let data = match tokscale::app_bin_dir() {
        Some(d) => d,
        None => {
            return Ok(TokscaleStatus {
                installed: false,
                version: None,
            })
        }
    };
    let custom = tokscale::bundled_bin_path(&app);
    let bin = match tokscale::resolve_bin(custom.as_deref(), &data) {
        Ok(b) => b,
        Err(_) => {
            return Ok(TokscaleStatus {
                installed: false,
                version: None,
            })
        }
    };
    let out = tokio::process::Command::new(&bin)
        .arg("--version")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let version = out
        .status
        .success()
        .then(|| {
            String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .last()
                .map(|v| v.to_string())
        })
        .flatten();
    Ok(TokscaleStatus {
        installed: true,
        version,
    })
}
