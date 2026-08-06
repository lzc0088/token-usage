//! Local SQLite storage (T2.1–T2.3). See docs/design.md §5.
//!
//! `daily_usage` / `sessions` are written by the collector (graph → upsert) and
//! read by the query layer (VM). The connection is opened + migrated via
//! [`open_db`]; callers share one connection (WAL allows concurrent reads).

pub mod daily_usage;
pub mod schema;
pub mod sessions;

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Columns allowed in a [`not_in_clause`] fragment. Hardcoded internal values
/// only — never accept caller-supplied column names from untrusted input.
const NOT_IN_ALLOWED_COLUMNS: &[&str] = &["tool", "session_id", "model", "date"];

/// Build a parameterized `NOT IN (...)` SQL fragment and matching param slice
/// from a list of values. Returns `(sql_fragment, params)` where `sql_fragment`
/// is e.g. `"tool NOT IN (?,?,?)"` or `"1=1"` when `items` is empty.
///
/// The returned fragment can be dropped directly into a WHERE clause:
/// `format!("WHERE {}", not_in_clause(&items).0)`.
///
/// # Safety
/// Values are bound via `?` placeholders and `column` is validated against a
/// hardcoded allowlist — no caller-supplied identifier is ever interpolated.
pub fn not_in_clause<'a, T: rusqlite::ToSql>(
    items: &'a [T],
    column: &'a str,
) -> Result<(String, Vec<&'a dyn rusqlite::ToSql>), StorageError> {
    if !NOT_IN_ALLOWED_COLUMNS.contains(&column) {
        return Err(StorageError::InvalidColumn(column.to_string()));
    }
    if items.is_empty() {
        return Ok(("1=1".into(), Vec::new()));
    }
    let placeholders = items.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("{column} NOT IN ({placeholders})");
    let params: Vec<&'a dyn rusqlite::ToSql> =
        items.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    Ok((sql, params))
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("storage io: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage parse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("storage: invalid column name for NOT IN clause: {0}")]
    InvalidColumn(String),
}

/// Where the app DB lives: `{data_local_dir}/token-usage/token-usage.db`.
pub fn db_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("token-usage").join("token-usage.db"))
}

/// Open (creating the parent dir if needed) and migrate the DB at `path`.
pub fn open_db(path: &Path) -> Result<Connection, StorageError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    schema::migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_db_creates_and_migrates_a_real_file() {
        let dir = std::env::temp_dir().join("tu_test_db");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("token-usage.db");
        let conn = open_db(&path).unwrap();
        let v: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, schema::CURRENT_VERSION);
        assert!(path.is_file(), "db file should exist");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── not_in_clause whitelist ──────────────────────────────────────────
    #[test]
    fn not_in_clause_accepts_known_columns() {
        let (sql, _) = not_in_clause(&["a", "b"], "tool").unwrap();
        assert_eq!(sql, "tool NOT IN (?,?)");
    }

    #[test]
    fn not_in_clause_rejects_unknown_column() {
        let result = not_in_clause(&["a", "b"], "malicious; DROP TABLE");
        assert!(result.is_err());
        if let Err(StorageError::InvalidColumn(col)) = result {
            assert!(col.contains("malicious"));
        } else {
            panic!("expected InvalidColumn error");
        }
    }

    #[test]
    fn not_in_clause_empty_returns_noop() {
        let (sql, params) = not_in_clause::<String>(&[], "tool").unwrap();
        assert_eq!(sql, "1=1");
        assert!(params.is_empty());
    }

    #[test]
    fn not_in_clause_empty_unknown_column_is_rejected() {
        // Empty list does NOT bypass the column whitelist — the column is
        // validated unconditionally so no caller can slip a malicious value in
        // through any code path.
        let result = not_in_clause::<String>(&[], "evil");
        assert!(result.is_err());
        if let Err(StorageError::InvalidColumn(col)) = result {
            assert_eq!(col, "evil");
        } else {
            panic!("expected InvalidColumn error");
        }
    }
}
