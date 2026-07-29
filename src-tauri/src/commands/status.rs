//! Tool-collection status (M3 T3.1). Async — spawns `tokscale clients`.

use serde::Serialize;

use crate::collector::tokscale;
use crate::utils::probe;
use crate::utils::paths;

/// One tool's tracking status, for the 采集 segment / hero tool dots.
#[derive(Debug, Clone, Serialize)]
pub struct ClientStatus {
    pub client: String,
    pub label: String,
    /// `active` (installed + has data) | `waiting` (installed, no data yet) | `missing`.
    pub status: &'static str,
    pub message_count: i64,
    /// Diagnostic notices from tokscale (v4.7.0+), e.g. unused stats-cache.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<ClientDiagnostic>,
}

/// A tokscale client diagnostic message.
#[derive(Debug, Clone, Serialize)]
pub struct ClientDiagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
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
            let installed = c.sessions_path_exists
                || c.message_count > 0
                || probe::is_installed(&c.client);
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
                diagnostics: c
                    .diagnostics
                    .iter()
                    .map(|d| ClientDiagnostic {
                        code: d.code.clone(),
                        severity: d.severity.clone(),
                        message: d.message.clone(),
                    })
                    .collect(),
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
