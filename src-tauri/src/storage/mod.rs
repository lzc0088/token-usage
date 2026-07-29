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

/// Build a parameterized `NOT IN (...)` SQL fragment and matching param slice
/// from a list of values. Returns `(sql_fragment, params)` where `sql_fragment`
/// is e.g. `"tool NOT IN (?,?,?)"` or `"1=1"` when `items` is empty.
///
/// The returned fragment can be dropped directly into a WHERE clause:
/// `format!("WHERE {}", not_in_clause(&items).0)`.
///
/// # Safety
/// All values are bound via `?` placeholders — never interpolated into SQL.
pub fn not_in_clause<'a, T: rusqlite::ToSql>(
    items: &'a [T],
    column: &'a str,
) -> (String, Vec<&'a dyn rusqlite::ToSql>) {
    if items.is_empty() {
        return ("1=1".into(), Vec::new());
    }
    let placeholders = items.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("{column} NOT IN ({placeholders})");
    let params: Vec<&'a dyn rusqlite::ToSql> = items.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    (sql, params)
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("storage io: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage parse: {0}")]
    Parse(#[from] serde_json::Error),
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
}
