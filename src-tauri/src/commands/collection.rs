//! Collection-tracking commands (采集追踪): archived-session count + clear.
//!
//! 归档会话 = sessions whose `tool` is no longer in the installed-clients list
//! (persisted to the `app_config` kv under `installed_clients` by the collector
//! runtime at startup). These commands read that list and delegate to
//! `storage::sessions` for the count / prune.

use tauri::{AppHandle, Emitter, State};

use crate::config;
use crate::state::AppState;
use crate::storage::sessions;

use super::db;

/// The installed-clients list cached at startup, or an empty vec if the
/// collector hasn't run / failed to persist it. An empty list makes every
/// session count as "archived" — acceptable fallback.
fn installed_clients(conn: &rusqlite::Connection) -> Vec<String> {
    config::get_json::<Vec<String>>(conn, "installed_clients").unwrap_or(None).unwrap_or_default()
}

/// Count retained sessions whose source tool is no longer installed.
#[tauri::command]
pub fn get_archived_session_count(state: State<AppState>) -> Result<i64, String> {
    let conn = db(&state);
    let installed = installed_clients(&conn);
    sessions::archived_count(&conn, &installed).map_err(|e| e.to_string())
}

/// Delete all retained sessions whose source tool is no longer installed.
/// Returns the number of rows deleted. Emits `collection:updated` so the
/// settings page can refresh the count live.
#[tauri::command]
pub fn clear_archived_sessions(
    app: AppHandle,
    state: State<AppState>,
) -> Result<usize, String> {
    let conn = db(&state);
    let installed = installed_clients(&conn);
    let deleted = sessions::prune_uninstalled(&conn, &installed).map_err(|e| e.to_string())?;
    let _ = app.emit("collection:updated", ());
    Ok(deleted)
}
