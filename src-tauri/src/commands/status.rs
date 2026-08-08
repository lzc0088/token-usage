//! Tool-collection status (M3 T3.1). Async — spawns `tokscale clients`.

use serde::{Deserialize, Serialize};

use crate::collector::tokscale;
use crate::utils::paths;
use crate::utils::probe;

/// One tool's tracking status, for the 采集 segment / hero tool dots.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            let installed =
                c.sessions_path_exists || c.message_count > 0 || probe::is_installed(&c.client);
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    let mut cmd = tokio::process::Command::new(&bin);
    cmd.arg("--version");
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW — suppress cmd/powershell flash
    let out = cmd
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

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure helper mirroring the status-resolution logic in `get_tools_status`
    /// (lines 42–50). Tests the decision matrix without spawning tokscale CLI.
    fn resolve_status(
        sessions_path_exists: bool,
        message_count: i64,
        probe_installed: bool,
    ) -> &'static str {
        let installed = sessions_path_exists || message_count > 0 || probe_installed;
        if message_count > 0 {
            "active"
        } else if installed {
            "waiting"
        } else {
            "missing"
        }
    }

    // ── ClientStatus serialization ───────────────────────────────────────

    #[test]
    fn client_status_serializes_roundtrip() {
        let cs = ClientStatus {
            client: "claude".into(),
            label: "Claude Code".into(),
            status: "active",
            message_count: 42,
            diagnostics: vec![],
        };
        let json = serde_json::to_value(&cs).unwrap();
        assert_eq!(json["client"], "claude");
        assert_eq!(json["label"], "Claude Code");
        assert_eq!(json["status"], "active");
        assert_eq!(json["message_count"], 42);
    }

    #[test]
    fn client_status_omits_empty_diagnostics() {
        let cs = ClientStatus {
            client: "codex".into(),
            label: "Codex CLI".into(),
            status: "missing",
            message_count: 0,
            diagnostics: vec![],
        };
        let json = serde_json::to_value(&cs).unwrap();
        assert!(
            json.get("diagnostics").is_none(),
            "empty diagnostics should be skipped"
        );
    }

    #[test]
    fn client_status_includes_diagnostics_when_present() {
        let cs = ClientStatus {
            client: "claude".into(),
            label: "Claude Code".into(),
            status: "active",
            message_count: 100,
            diagnostics: vec![ClientDiagnostic {
                code: "UNUSED_STATS_CACHE".into(),
                severity: "warn".into(),
                message: "stats-cache.json is present but unused".into(),
            }],
        };
        let json = serde_json::to_value(&cs).unwrap();
        let diags = &json["diagnostics"][0];
        assert_eq!(diags["code"], "UNUSED_STATS_CACHE");
    }

    // ── TokscaleStatus serialization ──────────────────────────────────────

    #[test]
    fn tokscale_status_installed_with_version() {
        let ts = TokscaleStatus {
            installed: true,
            version: Some("4.7.0".into()),
        };
        let json = serde_json::to_value(&ts).unwrap();
        assert_eq!(json["installed"], true);
        assert_eq!(json["version"], "4.7.0");
    }

    #[test]
    fn tokscale_status_not_installed() {
        let ts = TokscaleStatus {
            installed: false,
            version: None,
        };
        let json = serde_json::to_value(&ts).unwrap();
        assert_eq!(json["installed"], false);
        assert!(json["version"].is_null());
    }

    // ── Status resolution logic ─────────────────────────────────────────

    #[test]
    fn status_active_when_message_count_positive() {
        assert_eq!(resolve_status(false, 1, false), "active");
        assert_eq!(resolve_status(true, 5, false), "active");
        assert_eq!(resolve_status(false, 100, true), "active");
    }

    #[test]
    fn status_waiting_when_installed_but_no_messages() {
        assert_eq!(resolve_status(true, 0, false), "waiting");
        assert_eq!(resolve_status(false, 0, true), "waiting");
        assert_eq!(resolve_status(true, 0, true), "waiting");
    }

    #[test]
    fn status_missing_when_not_installed_and_no_messages() {
        assert_eq!(resolve_status(false, 0, false), "missing");
    }

    // ── ClientDiagnostic serialization ───────────────────────────────────

    #[test]
    fn client_diagnostic_serializes_all_fields() {
        let d = ClientDiagnostic {
            code: "STALE_CACHE".into(),
            severity: "error".into(),
            message: "Cache is older than 30 days".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: ClientDiagnostic = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, "STALE_CACHE");
        assert_eq!(back.severity, "error");
        assert_eq!(back.message, "Cache is older than 30 days");
    }
}
