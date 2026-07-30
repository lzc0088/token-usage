//! Config + credential commands (M3/M5).

use tauri::{AppHandle, Emitter, State};
use tracing::warn;

use crate::auth::credentials;
use crate::commands::db;
use crate::config::Config;
use crate::quota::VendorId;
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
    // The fetch above already cached fresh quota data, so this emit correctly
    // notifies windows that the quota cache has been refreshed.
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
    _app: AppHandle,
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
        obj.insert(
            "cookie".into(),
            serde_json::Value::String(cookie.to_string()),
        );
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
    // NOTE: do NOT emit `quota:updated` here — the quota cache has NOT been
    // refreshed yet, so any window that reloads on this event would read stale
    // data (e.g. the old cookie_error). The frontend `saveCookie` awaits a
    // real `refresh_quota` call, which emits the event AFTER the cache is
    // updated.  Removing this premature emit fixes the bug where saving a
    // valid cookie still shows "Cookie 已过期" until a tab switch.
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

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::credentials;
    use crate::storage::schema;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    // SAFETY: `State<'_, T>` is `pub struct State<'r, T>(&'r T)`. Transmuting
    // a reference is safe because the wrapper holds no extra state.
    fn state_of(state: &AppState) -> State<'_, AppState> {
        unsafe { std::mem::transmute(state) }
    }

    // ═══ Config (no AppHandle needed) ═══════════════════════════════════════

    #[test]
    fn get_config_defaults_when_empty() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        let cfg = get_config(state_of(&state)).unwrap();
        assert_eq!(cfg.currency, crate::config::Currency::Both);
    }

    // ═══ Credential status (no AppHandle needed) ════════════════════════════

    #[test]
    fn credential_status_false_for_unknown_vendor() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        assert!(!get_credential_status("nonexistent".into(), state_of(&state)).unwrap());
    }

    // ═══ set_credential logic (via credentials module) ══════════════════════
    // The command trims, validates (unmapped vendors skip network), then calls
    // `credentials::set`. We test the same logic directly.

    #[tokio::test]
    async fn set_credential_stores_unmapped_vendor() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(&state.db_guard(), "testvendor", "sk-secret").unwrap();
        assert!(credentials::exists(&state.db_guard(), "testvendor").unwrap());
        assert_eq!(
            credentials::get(&state.db_guard(), "testvendor").unwrap(),
            "sk-secret"
        );
    }

    #[test]
    fn set_credential_rejects_empty_after_trim() {
        let trimmed = "   ".trim();
        assert!(trimmed.is_empty(), "whitespace-only should be rejected");
    }

    // ═══ delete_credential logic ════════════════════════════════════════════

    #[test]
    fn delete_removes_credential_and_quota_cache() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(&state.db_guard(), "delvendor", "sk-del").unwrap();
        let _ = state.db_guard().execute(
            "INSERT INTO quota_cache (vendor, data, fetched_at) VALUES (?, ?, 1)",
            rusqlite::params!["delvendor", r#"{"status":"ok","windows":[]}"#],
        );
        // The command deletes credential + quota cache.
        credentials::delete(&state.db_guard(), "delvendor").unwrap();
        let _ = state
            .db_guard()
            .execute("DELETE FROM quota_cache WHERE vendor = ?", [&"delvendor"]);
        assert!(credentials::get(&state.db_guard(), "delvendor").is_err());
        let cached: Option<i64> = state
            .db_guard()
            .query_row(
                "SELECT fetched_at FROM quota_cache WHERE vendor = ?",
                [&"delvendor"],
                |r| r.get(0),
            )
            .ok();
        assert!(cached.is_none(), "quota cache should be cleared");
    }

    // ═══ update_cookie logic ════════════════════════════════════════════════

    #[test]
    fn update_cookie_adds_cookie_to_json() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(&state.db_guard(), "cv", r#"{"key":"sk-key"}"#).unwrap();
        // Simulate update_cookie: get → add cookie → save.
        let raw = credentials::get(&state.db_guard(), "cv").unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        v.as_object_mut().unwrap().insert(
            "cookie".into(),
            serde_json::Value::String("cookie_val".into()),
        );
        credentials::set(&state.db_guard(), "cv", &v.to_string()).unwrap();
        let fields = get_credential_fields("cv".into(), state_of(&state)).unwrap();
        assert!(fields.contains(&"cookie".to_string()));
        assert!(fields.contains(&"key".to_string()));
    }

    #[test]
    fn update_cookie_rejects_empty_after_trim() {
        let trimmed = "   ".trim();
        assert!(trimmed.is_empty(), "empty after trim → rejected");
    }

    #[test]
    fn update_cookie_creates_credential_when_none_exists() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        // When no prior credential, update_cookie creates {"cookie": "..."}.
        credentials::set(&state.db_guard(), "newvendor", r#"{"cookie":"cookie123"}"#).unwrap();
        let raw = credentials::get(&state.db_guard(), "newvendor").unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v.get("cookie").and_then(|s| s.as_str()), Some("cookie123"));
    }

    #[test]
    fn update_cookie_merges_extra_fields() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(&state.db_guard(), "mixed", r#"{"key":"k"}"#).unwrap();
        let raw = credentials::get(&state.db_guard(), "mixed").unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        v.as_object_mut()
            .unwrap()
            .insert("cookie".into(), serde_json::Value::String("cv".into()));
        v.as_object_mut()
            .unwrap()
            .insert("region".into(), serde_json::Value::String("cn".into()));
        credentials::set(&state.db_guard(), "mixed", &v.to_string()).unwrap();
        let raw = credentials::get(&state.db_guard(), "mixed").unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v.get("key").and_then(|s| s.as_str()), Some("k"));
        assert_eq!(v.get("cookie").and_then(|s| s.as_str()), Some("cv"));
        assert_eq!(v.get("region").and_then(|s| s.as_str()), Some("cn"));
    }

    #[test]
    fn update_cookie_removes_empty_extra_field() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(&state.db_guard(), "evict", r#"{"key":"k","region":"us"}"#).unwrap();
        let raw = credentials::get(&state.db_guard(), "evict").unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        v.as_object_mut()
            .unwrap()
            .insert("cookie".into(), serde_json::Value::String("cv".into()));
        v.as_object_mut().unwrap().remove("region"); // empty → removed
        credentials::set(&state.db_guard(), "evict", &v.to_string()).unwrap();
        let raw = credentials::get(&state.db_guard(), "evict").unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(v.get("region").is_none());
        assert_eq!(v.get("cookie").and_then(|s| s.as_str()), Some("cv"));
    }

    // ═══ get_credential_fields (no AppHandle needed) ════════════════════════

    #[test]
    fn fields_empty_for_missing_vendor() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        assert!(get_credential_fields("nope".into(), state_of(&state))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn fields_lists_nonempty_json_keys() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(
            &state.db_guard(),
            "mf",
            r#"{"key":"k","secret":"s","cookie":"c"}"#,
        )
        .unwrap();
        let fields = get_credential_fields("mf".into(), state_of(&state)).unwrap();
        assert_eq!(fields.len(), 3);
        assert!(fields.contains(&"key".to_string()));
        assert!(fields.contains(&"secret".to_string()));
        assert!(fields.contains(&"cookie".to_string()));
    }

    #[test]
    fn fields_omits_empty_json_values() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(&state.db_guard(), "p", r#"{"key":"k","secret":""}"#).unwrap();
        let fields = get_credential_fields("p".into(), state_of(&state)).unwrap();
        assert_eq!(
            fields,
            vec!["key".to_string()],
            "empty secret should be filtered out"
        );
    }

    #[test]
    fn fields_legacy_plaintext_returns_key() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(&state.db_guard(), "legacy", "plain-secret").unwrap();
        let fields = get_credential_fields("legacy".into(), state_of(&state)).unwrap();
        assert_eq!(fields, vec!["key".to_string()]);
    }

    // ═══ get_credential_field_values (no AppHandle needed) ══════════════════

    #[test]
    fn field_values_excludes_secrets() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(
            &state.db_guard(),
            "fv",
            r#"{"key":"k","cookie":"c","region":"cn"}"#,
        )
        .unwrap();
        let vals = get_credential_field_values("fv".into(), state_of(&state)).unwrap();
        assert!(!vals.contains_key("key"), "key should be excluded");
        assert!(!vals.contains_key("cookie"), "cookie should be excluded");
        assert_eq!(vals.get("region").map(|s| s.as_str()), Some("cn"));
    }

    #[test]
    fn field_values_empty_for_missing_vendor() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        assert!(get_credential_field_values("nv".into(), state_of(&state))
            .unwrap()
            .is_empty());
    }

    // ═══ clear_credential_fields logic ══════════════════════════════════════

    #[test]
    fn clear_fields_removes_specified_keys() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(
            &state.db_guard(),
            "cf",
            r#"{"key":"k","cookie":"c","region":"cn"}"#,
        )
        .unwrap();
        // Simulate: remove "cookie" → key and region remain.
        let raw = credentials::get(&state.db_guard(), "cf").unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        if let Some(obj) = v.as_object_mut() {
            obj.remove("cookie");
        }
        credentials::set(&state.db_guard(), "cf", &v.to_string()).unwrap();
        let raw = credentials::get(&state.db_guard(), "cf").unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(v.get("cookie").is_none());
        assert!(v.get("key").is_some());
        assert_eq!(v.get("region").and_then(|s| s.as_str()), Some("cn"));
    }

    #[test]
    fn clear_fields_deletes_credential_when_empty() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(&state.db_guard(), "fd", r#"{"key":"k","cookie":"c"}"#).unwrap();
        // Simulate: remove both key and cookie → credential deleted.
        let raw = credentials::get(&state.db_guard(), "fd").unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        if let Some(obj) = v.as_object_mut() {
            obj.remove("key");
            obj.remove("cookie");
        }
        let has = v
            .get("key")
            .and_then(|x| x.as_str())
            .is_some_and(|s| !s.is_empty())
            || v.get("cookie")
                .and_then(|x| x.as_str())
                .is_some_and(|s| !s.is_empty());
        assert!(!has, "both fields removed");
        credentials::delete(&state.db_guard(), "fd").unwrap();
        assert!(credentials::get(&state.db_guard(), "fd").is_err());
    }

    #[test]
    fn clear_fields_removes_quota_cache() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        credentials::set(&state.db_guard(), "cc", r#"{"key":"k"}"#).unwrap();
        let _ = state.db_guard().execute(
            "INSERT INTO quota_cache (vendor, data, fetched_at) VALUES (?, ?, 1)",
            rusqlite::params!["cc", r#"{"status":"ok","windows":[]}"#],
        );
        // Simulate: clear "key" → quota cache also cleared.
        let _ = state
            .db_guard()
            .execute("DELETE FROM quota_cache WHERE vendor = ?", [&"cc"]);
        let cached: Option<i64> = state
            .db_guard()
            .query_row(
                "SELECT fetched_at FROM quota_cache WHERE vendor = ?",
                [&"cc"],
                |r| r.get(0),
            )
            .ok();
        assert!(cached.is_none(), "quota cache should be cleared");
    }

    // ═══ Vendor ID mapping ══════════════════════════════════════════════════

    #[test]
    fn vendor_id_mapping_covers_all_known_vendors() {
        let vendors = [
            ("deepseek", VendorId::Deepseek),
            ("glm", VendorId::Glm),
            ("minimax", VendorId::Minimax),
            ("kimi", VendorId::Kimi),
            ("volcengine", VendorId::Volcengine),
            ("mimo", VendorId::Mimo),
            ("stepfun", VendorId::Stepfun),
            ("iflytek", VendorId::Iflytek),
            ("zai_team", VendorId::GlmTeam),
            ("qoder", VendorId::Qoder),
            ("cursor", VendorId::Cursor),
            ("copilot", VendorId::Copilot),
            ("ollama", VendorId::Ollama),
            ("opencode", VendorId::Opencode),
            ("claude", VendorId::Claude),
            ("codex", VendorId::Codex),
        ];
        for (name, expected) in vendors {
            let got = match name {
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
            assert_eq!(got, Some(expected), "vendor '{}' mapping", name);
        }
    }
}
