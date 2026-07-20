//! SQLite schema + migrations (T2.1).
//!
//! Four tables (design.md §5.1):
//!   - `daily_usage`  — per-day × tool × model token breakdown (from `tokscale graph`)
//!   - `sessions`     — per-session aggregates
//!   - `collection_state` — anchor / last-full-scan / scheduler kv
//!   - `app_config`   — user settings kv (currency, module visibility, tokscale path…)
//!
//! Migrations are versioned via `PRAGMA user_version` and idempotent. WAL mode is
//! enabled so collection writes don't block query reads (design §11 risk).

use rusqlite::Connection;

use super::StorageError;

/// Current schema version. Bump + add a migration step on schema change.
pub const CURRENT_VERSION: u32 = 2;

/// v1: initial tables + indexes.
const V1: &str = r#"
-- Per-day usage breakdown (supports any date-range query; core need vs token-monitor's JSON).
CREATE TABLE IF NOT EXISTS daily_usage (
  date           TEXT    NOT NULL,            -- YYYY-MM-DD (local tz)
  tool           TEXT    NOT NULL,            -- claude / codex / opencode / zcode ...
  model          TEXT    NOT NULL,            -- glm-5.2 / step-3.7-flash ...
  input_tokens   INTEGER NOT NULL DEFAULT 0,
  output_tokens  INTEGER NOT NULL DEFAULT 0,
  cache_read_tokens  INTEGER NOT NULL DEFAULT 0,
  cache_write_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_tokens   INTEGER NOT NULL DEFAULT 0,  -- stored separately; NOT in total (anti double-count)
  cost_usd       REAL    NOT NULL DEFAULT 0,
  messages       INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (date, tool, model)
);
CREATE INDEX IF NOT EXISTS idx_daily_date  ON daily_usage(date);
CREATE INDEX IF NOT EXISTS idx_daily_tool  ON daily_usage(tool, date);
CREATE INDEX IF NOT EXISTS idx_daily_model ON daily_usage(model, date);

-- Per-session aggregates.
CREATE TABLE IF NOT EXISTS sessions (
  tool               TEXT    NOT NULL,
  session_id         TEXT    NOT NULL,
  model              TEXT    NOT NULL,
  started_at         INTEGER,                 -- epoch ms
  last_used_at       INTEGER,
  project_path       TEXT,
  input_tokens       INTEGER NOT NULL DEFAULT 0,
  output_tokens      INTEGER NOT NULL DEFAULT 0,
  cache_read_tokens  INTEGER NOT NULL DEFAULT 0,
  cache_write_tokens INTEGER NOT NULL DEFAULT 0,
  cost_usd           REAL    NOT NULL DEFAULT 0,
  PRIMARY KEY (tool, session_id, model)
);
CREATE INDEX IF NOT EXISTS idx_session_lastused ON sessions(last_used_at);
CREATE INDEX IF NOT EXISTS idx_session_project  ON sessions(project_path);

-- Collector kv (anchor snapshot, lastFullScanAt, configFingerprint…).
CREATE TABLE IF NOT EXISTS collection_state (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

-- User settings kv (currency, module_visibility, tool_display_order, enabled_tools…).
CREATE TABLE IF NOT EXISTS app_config (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,            -- JSON-encoded value
  updated_at INTEGER NOT NULL
);
"#;

/// v2: add `message_count` to sessions so the query layer can surface message
/// counts without another tokscale round-trip.
const V2: &str = r#"
ALTER TABLE sessions ADD COLUMN message_count INTEGER NOT NULL DEFAULT 0;
"#;

/// Apply all pending migrations to `conn`. Idempotent.
pub fn migrate(conn: &Connection) -> Result<(), StorageError> {
    // WAL: collection writes vs query reads (design §11).
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    let current: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current < 1 {
        conn.execute_batch(V1)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    if current < 2 {
        conn.execute_batch(V2)?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn migrate_creates_all_tables() {
        let conn = fresh();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for expected in ["daily_usage", "sessions", "collection_state", "app_config"] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table {expected}: {tables:?}"
            );
        }
    }

    #[test]
    fn migrate_sets_user_version() {
        let conn = fresh();
        let v: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, CURRENT_VERSION);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        // second run must not error and must keep version stable
        migrate(&conn).unwrap();
        let v: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, CURRENT_VERSION);
    }

    #[test]
    fn indexes_created() {
        let conn = fresh();
        let idxs: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for expected in [
            "idx_daily_date",
            "idx_daily_tool",
            "idx_daily_model",
            "idx_session_lastused",
            "idx_session_project",
        ] {
            assert!(
                idxs.contains(&expected.to_string()),
                "missing index {expected}"
            );
        }
    }

    #[test]
    fn reasoning_stored_separately_from_total() {
        // sanity: the column exists and total = input+output+cache_read+cache_write
        // (reasoning excluded — anti double-count, design §5.3).
        let conn = fresh();
        conn.execute(
            "INSERT INTO daily_usage (date,tool,model,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,reasoning_tokens,cost_usd,messages)
             VALUES ('2026-07-17','claude','glm-5.2',100,20,50,0,999,1.5,3)",
            [],
        )
        .unwrap();
        let total: i64 = conn
            .query_row(
                "SELECT input_tokens+output_tokens+cache_read_tokens+cache_write_tokens FROM daily_usage WHERE tool='claude'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 170); // 100+20+50+0, reasoning(999) excluded
    }
}
