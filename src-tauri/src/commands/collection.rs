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

use super::{db, db_write};

/// The installed-clients list cached at startup, or an empty vec if the
/// collector hasn't run / failed to persist it. An empty list makes every
/// session count as "archived" — acceptable fallback.
fn installed_clients(conn: &rusqlite::Connection) -> Vec<String> {
    config::get_json::<Vec<String>>(conn, "installed_clients")
        .unwrap_or(None)
        .unwrap_or_default()
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
pub fn clear_archived_sessions(app: AppHandle, state: State<AppState>) -> Result<usize, String> {
    let conn = db_write(&state);
    let installed = installed_clients(&conn);
    let deleted = sessions::prune_uninstalled(&conn, &installed).map_err(|e| e.to_string())?;
    let _ = app.emit("collection:updated", ());
    Ok(deleted)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::storage::{schema, sessions};
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    /// SAFETY: `State<'_, T>` is `pub struct State<'r, T>(&'r T)`.
    fn state_of(state: &AppState) -> State<'_, AppState> {
        unsafe { std::mem::transmute(state) }
    }

    fn insert_session(conn: &Connection, tool: &str, session_id: &str, model: &str) {
        conn.execute(
            "INSERT INTO sessions (tool, session_id, model, input_tokens, output_tokens,
             cache_read_tokens, cache_write_tokens, cost_usd, message_count)
             VALUES (?1, ?2, ?3, 100, 50, 0, 0, 0.01, 3)",
            rusqlite::params![tool, session_id, model],
        )
        .unwrap();
    }

    // ── installed_clients ────────────────────────────────────────────────

    #[test]
    fn installed_clients_returns_empty_when_no_data() {
        let conn = mem();
        let clients = installed_clients(&conn);
        assert!(clients.is_empty());
    }

    #[test]
    fn installed_clients_returns_stored_list() {
        let conn = mem();
        let expected = vec!["claude".to_string(), "codex".to_string()];
        config::set_json(&conn, "installed_clients", &expected).unwrap();
        let clients = installed_clients(&conn);
        assert_eq!(clients, expected);
    }

    #[test]
    fn installed_clients_returns_empty_when_corrupted_json() {
        let conn = mem();
        config::set_raw(&conn, "installed_clients", "{not valid json").unwrap();
        let clients = installed_clients(&conn);
        assert!(clients.is_empty());
    }

    // ── get_archived_session_count ────────────────────────────────────────

    #[test]
    fn archived_count_zero_with_no_clients_and_no_sessions() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        let count = get_archived_session_count(state_of(&state)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn archived_count_zero_when_all_tools_installed() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_guard();
            config::set_json(
                &conn,
                "installed_clients",
                &vec!["claude".to_string(), "codex".to_string()],
            )
            .unwrap();
            insert_session(&conn, "claude", "s1", "gpt-5");
            insert_session(&conn, "codex", "s2", "gpt-5");
        }
        let count = get_archived_session_count(state_of(&state)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn archived_count_counts_uninstalled_tool_sessions() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_guard();
            config::set_json(&conn, "installed_clients", &vec!["claude".to_string()]).unwrap();
            insert_session(&conn, "claude", "s1", "gpt-5");
            insert_session(&conn, "codex", "s2", "gpt-5");
            insert_session(&conn, "codex", "s3", "gpt-5-plus");
        }
        let count = get_archived_session_count(state_of(&state)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn archived_count_zero_when_installed_list_empty() {
        let state = AppState::with_db(Arc::new(Mutex::new(mem())));
        {
            let conn = state.db_guard();
            insert_session(&conn, "claude", "s1", "gpt-5");
            insert_session(&conn, "codex", "s2", "gpt-5");
            insert_session(&conn, "cursor", "s3", "gpt-5");
        }
        // No installed_clients in KV → installed_clients() returns [] → 0 archived.
        let count = get_archived_session_count(state_of(&state)).unwrap();
        assert_eq!(count, 0);
    }

    // ── prune_uninstalled integration ─────────────────────────────────────

    #[test]
    fn prune_uninstalled_clears_only_uninstalled() {
        let conn = mem();
        config::set_json(
            &conn,
            "installed_clients",
            &vec!["claude".to_string(), "cursor".to_string()],
        )
        .unwrap();
        insert_session(&conn, "claude", "s1", "m1");
        insert_session(&conn, "claude", "s1", "m2");
        insert_session(&conn, "codex", "s2", "m1");
        insert_session(&conn, "cursor", "s3", "m1");

        let installed = installed_clients(&conn);
        let deleted = sessions::prune_uninstalled(&conn, &installed).unwrap();
        assert_eq!(deleted, 1); // only codex

        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 3);
    }

    #[test]
    fn prune_uninstalled_noop_when_empty_installed_list() {
        let conn = mem();
        let empty: Vec<String> = vec![];
        config::set_json(&conn, "installed_clients", &empty).unwrap();
        insert_session(&conn, "claude", "s1", "m1");
        insert_session(&conn, "codex", "s2", "m1");

        let installed = installed_clients(&conn);
        assert!(installed.is_empty());
        let deleted = sessions::prune_uninstalled(&conn, &installed).unwrap();
        assert_eq!(deleted, 0);

        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 2);
    }

    #[test]
    fn prune_uninstalled_noop_when_no_sessions() {
        let conn = mem();
        config::set_json(&conn, "installed_clients", &vec!["claude".to_string()]).unwrap();
        let installed = installed_clients(&conn);
        let deleted = sessions::prune_uninstalled(&conn, &installed).unwrap();
        assert_eq!(deleted, 0);
    }
}
