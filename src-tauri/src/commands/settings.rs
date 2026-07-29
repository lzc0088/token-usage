//! Config + credential commands (M3/M5).

use tauri::{AppHandle, Emitter, State};
use tracing::warn;

use crate::commands::db;
use crate::config::Config;
use crate::auth::credentials;
use crate::quota::{copilot, VendorId};
use crate::state::AppState;

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Result<Config, String> {
    crate::config::load(&db(&state)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_config(config: Config, state: State<AppState>, app: AppHandle) -> Result<(), String> {
    let conn = db(&state);
    crate::config::save(&conn, &config).map_err(|e| e.to_string())?;
    // Apply window-behaviour settings live (dock, drag, hotkey, tray).
    crate::ui::window::apply_window_config(&app, &conn);
    // Notify all windows (e.g. the main popover) so layout/currency changes
    // apply live without waiting for a manual refresh.
    let _ = app.emit("config:changed", ());
    Ok(())
}

/// Check if a vendor has a stored credential (encrypted in the local DB).
///
/// Returns `Err` on DB failure so the UI can surface the problem instead of
/// silently treating a DB error as "not bound".
#[tauri::command]
pub fn get_credential_status(vendor: String, state: State<AppState>) -> Result<bool, String> {
    let conn = db(&state);
    credentials::exists(&conn, &vendor).map_err(|e| e.to_string())
}

/// Store a vendor credential (encrypted in the local DB).
/// Validates the credential by making a test API call before saving.
#[tauri::command]
pub async fn set_credential(
    vendor: String,
    secret: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    // Trim surrounding whitespace/newlines — common when pasting from browsers.
    let secret = secret.trim();
    if secret.is_empty() {
        return Err("请填写凭证".into());
    }

    // Validate credential for vendors with a quota adapter.
    let vid = match vendor.as_str() {
        "deepseek" => Some(VendorId::Deepseek),
        "glm" => Some(VendorId::Glm),
        "minimax" => Some(VendorId::Minimax),
        "kimi" => Some(VendorId::Kimi),
        "volcengine" => Some(VendorId::Volcengine),
        "mimo" => Some(VendorId::Mimo),
        "stepfun" => Some(VendorId::Stepfun),
        "iflytek" => Some(VendorId::Iflytek),
        "zai_team" => Some(VendorId::GlmTeam),
        "qoder" => Some(VendorId::Qoder),
        "cursor" => Some(VendorId::Cursor),
        "copilot" => Some(VendorId::Copilot),
        "ollama" => Some(VendorId::Ollama),
        "opencode" => Some(VendorId::Opencode),
        "claude" => Some(VendorId::Claude),
        "codex" => Some(VendorId::Codex),
        _ => None,
    };

    // For vendors with quota adapters, fetch and cache the quota on save.
    if let Some(vid) = vid {
        match crate::quota::fetch(vid, secret).await {
            Ok(mut q) => {
                q.refreshed_at = Some(chrono::Utc::now().to_rfc3339());
                let conn = state.db_guard();
                crate::quota::scheduler::write_cache(
                    &conn,
                    &vendor,
                    &q,
                    chrono::Utc::now().timestamp_millis(),
                );
            }
            Err(e) => {
                let msg = e.to_string();
                warn!(vendor = %vendor, error = %msg, "set_credential quota fetch failed");
                return Err(crate::quota::format_validate_error(&msg));
            }
        }
    }

    let conn = db(&state);
    credentials::set(&conn, &vendor, secret).map_err(|e| e.to_string())?;
    // Notify all windows so the 额度 page + 总览 reload the freshly-cached quota.
    let _ = app.emit("quota:updated", ());
    Ok(())
}

/// Delete a vendor credential. Also removes its cached quota so the limits
/// page stops showing it immediately.
#[tauri::command]
pub fn delete_credential(
    vendor: String,
    state: State<AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let conn = db(&state);
    credentials::delete(&conn, &vendor).map_err(|e| e.to_string())?;
    // Remove cached quota so the limits page no longer shows this vendor.
    let _ = conn.execute(
        "DELETE FROM quota_cache WHERE vendor = ?",
        rusqlite::params![vendor],
    );
    // Notify windows to drop this vendor from the quota lists immediately.
    let _ = app.emit("quota:updated", ());
    Ok(())
}

/// Update only the `cookie` field (and optionally other fields like `region`)
/// of an already-stored credential, preserving any other fields (e.g.
/// Volcengine's key/secret). For cookie-only vendors whose stored credential
/// is a plain cookie string, it is wrapped as `{"cookie": ...}`. No
/// re-validation here — the frontend triggers a refresh after this so the card
/// updates live.
#[tauri::command]
pub fn update_cookie(
    vendor: String,
    cookie: String,
    extra_fields: Option<std::collections::HashMap<String, String>>,
    state: State<AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let cookie = cookie.trim();
    if cookie.is_empty() {
        return Err("请填写 Cookie".into());
    }
    let conn = db(&state);
    let mut base = match credentials::get(&conn, &vendor).ok() {
        Some(existing) => match serde_json::from_str::<serde_json::Value>(&existing) {
            Ok(v) if v.is_object() => v,
            // Non-JSON credential → wrap as {cookie: <old>}.
            Ok(other) => serde_json::json!({ "cookie": other }),
            _ => serde_json::json!({}),
        },
        None => serde_json::json!({}),
    };
    {
        let obj = base.as_object_mut().expect("base is an object");
        obj.insert("cookie".into(), serde_json::Value::String(cookie.to_string()));
        // Merge any extra fields (e.g. region/site) supplied by the editor.
        if let Some(extra) = extra_fields {
            for (k, v) in extra {
                let v = v.trim();
                if v.is_empty() {
                    obj.remove(&k);
                } else {
                    obj.insert(k, serde_json::Value::String(v.to_string()));
                }
            }
        }
    }
    credentials::set(&conn, &vendor, &base.to_string()).map_err(|e| e.to_string())?;
    // Notify windows so the quota card reloads after the editor closes.
    let _ = app.emit("quota:updated", ());
    Ok(())
}

/// Return the non-empty field names in a vendor's stored credential (e.g.
/// `["key","secret","cookie"]`). Empty vec when nothing is stored. The Account
/// page uses this to show per-field "clear" buttons for mixed vendors
/// (Volcengine: key + cookie) and to drive the bound/unbound badge.
#[tauri::command]
pub fn get_credential_fields(
    vendor: String,
    state: State<AppState>,
) -> Result<Vec<String>, String> {
    let conn = db(&state);
    let raw = match credentials::get(&conn, &vendor) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]),
    };
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(serde_json::Value::Object(m)) => Ok(m
            .into_iter()
            .filter(|(_, v)| v.as_str().is_some_and(|s| !s.is_empty()))
            .map(|(k, _)| k)
            .collect()),
        // Legacy plain-string credential → treat as a single "key" field.
        _ => Ok(vec!["key".into()]),
    }
}

/// Return the values of NON-SECRET scalar fields (region, site, projid, …)
/// from a stored credential. Used by the inline cookie editor to pre-fill a
/// region selector. Secret fields (key/secret/cookie) are never returned.
#[tauri::command]
pub fn get_credential_field_values(
    vendor: String,
    state: State<AppState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    // Secret field keys — never exposed to the frontend.
    const SECRET_KEYS: &[&str] = &["key", "secret", "cookie"];
    let conn = db(&state);
    let raw = match credentials::get(&conn, &vendor) {
        Ok(s) => s,
        Err(_) => return Ok(std::collections::HashMap::new()),
    };
    let mut out = std::collections::HashMap::new();
    if let Ok(serde_json::Value::Object(m)) = serde_json::from_str::<serde_json::Value>(&raw) {
        for (k, v) in m {
            if SECRET_KEYS.contains(&k.as_str()) {
                continue;
            }
            if let Some(s) = v.as_str() {
                if !s.is_empty() {
                    out.insert(k, s.to_string());
                }
            }
        }
    }
    Ok(out)
}

/// Remove specific fields from a vendor's stored credential (e.g. drop
/// "cookie" while keeping "key"/"secret"). If, after removal, neither a key
/// nor a cookie remains, the whole credential is deleted (it could not fetch
/// anything). The vendor's cached quota is cleared so the next refresh
/// re-fetches with the updated credential.
#[tauri::command]
pub fn clear_credential_fields(
    vendor: String,
    fields: Vec<String>,
    state: State<AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let conn = db(&state);
    let raw = credentials::get(&conn, &vendor).map_err(|e| e.to_string())?;
    let mut v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("credential parse: {e}"))?;
    if let Some(obj) = v.as_object_mut() {
        for f in &fields {
            obj.remove(f);
        }
    }
    let has_key = v
        .get("key")
        .and_then(|x| x.as_str())
        .is_some_and(|s| !s.is_empty());
    let has_cookie = v
        .get("cookie")
        .and_then(|x| x.as_str())
        .is_some_and(|s| !s.is_empty());

    // Drop cached quota so the next refresh re-fetches with updated creds.
    let _ = conn.execute(
        "DELETE FROM quota_cache WHERE vendor = ?",
        rusqlite::params![vendor],
    );

    if !has_key && !has_cookie {
        credentials::delete(&conn, &vendor).map_err(|e| e.to_string())?;
    } else {
        credentials::set(&conn, &vendor, &v.to_string()).map_err(|e| e.to_string())?;
    }
    // Notify windows to reflect the cleared/updated credential immediately.
    let _ = app.emit("quota:updated", ());
    Ok(())
}

/// Run the GitHub Copilot OAuth Device Flow.
///
/// Emits `copilot:login_status` events as the flow progresses:
///   `{phase: "authorize", user_code, verification_url, expires_in}` — frontend
///   opens the browser (shell plugin) and shows the user code.
///   `{phase: "polling"}` / `{phase: "success"}` — progress updates.
/// Returns the GitHub access token on success; the frontend then stores it via
/// `set_credential("copilot", token)`.
#[tauri::command]
pub async fn copilot_login(app: AppHandle) -> Result<String, String> {
    copilot::run_device_flow(&app)
        .await
        .map_err(|e| crate::quota::format_validate_error(&e.to_string()))
}
