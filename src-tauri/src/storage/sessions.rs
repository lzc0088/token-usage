//! Ingest `tokscale --group-by session,model` output into `sessions` (T2.2).
//!
//! Like `daily_usage`, tokscale reports absolute per-session totals; upsert =
//! replace-on-conflict, not accumulate. `started_at` / `last_used_at` /
//! `project_path` are nullable — tokscale doesn't surface them per-session in
//! the report shape, so V1 leaves them NULL and the sessions view falls back
//! to token/cost/model attribution only.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Deserialize;

use super::StorageError;

/// One row destined for `sessions`.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub tool: String,
    pub session_id: String,
    pub model: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub cost_usd: f64,
    pub message_count: i64,
    pub last_used_at: i64,
}

// ── `tokscale --group-by session,model` JSON shape ──────────────────────────

#[derive(Debug, Deserialize)]
pub struct SessionsReport {
    #[serde(default)]
    pub entries: Vec<SessionEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEntry {
    pub client: String,
    pub session_id: String,
    pub model: String,
    #[serde(default)]
    pub input: i64,
    #[serde(default)]
    pub output: i64,
    #[serde(default)]
    pub cache_read: i64,
    #[serde(default)]
    pub cache_write: i64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub message_count: i64,
}

/// Pure: flatten `entries[]` → session rows.
pub fn rows_from_report(r: &SessionsReport) -> Vec<SessionRow> {
    let now = Utc::now().timestamp_millis();
    r.entries
        .iter()
        .map(|e| SessionRow {
            tool: e.client.clone(),
            session_id: e.session_id.clone(),
            model: e.model.clone(),
            input: e.input,
            output: e.output,
            cache_read: e.cache_read,
            cache_write: e.cache_write,
            cost_usd: e.cost,
            message_count: e.message_count,
            last_used_at: now,
        })
        .collect()
}

/// Upsert (replace-on-conflict) session rows. Single transaction.
pub fn upsert_rows(conn: &mut Connection, rows: &[SessionRow]) -> Result<usize, StorageError> {
    if rows.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction()?;
    let mut n = 0;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO sessions
               (tool, session_id, model, input_tokens, output_tokens,
                cache_read_tokens, cache_write_tokens, cost_usd,
                message_count, last_used_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(tool, session_id, model) DO UPDATE SET
               input_tokens       = excluded.input_tokens,
               output_tokens      = excluded.output_tokens,
               cache_read_tokens  = excluded.cache_read_tokens,
               cache_write_tokens = excluded.cache_write_tokens,
               cost_usd           = excluded.cost_usd,
               message_count      = excluded.message_count",
            // NOTE: last_used_at is deliberately NOT updated on conflict —
            // it records first-seen time; real last-interaction time comes from
            // the session file mtime (see workspace::session_project_map).
        )?;
        for r in rows {
            stmt.execute(params![
                r.tool,
                r.session_id,
                r.model,
                r.input,
                r.output,
                r.cache_read,
                r.cache_write,
                r.cost_usd,
                r.message_count,
                r.last_used_at,
            ])?;
            n += 1;
        }
    }
    tx.commit()?;
    Ok(n)
}

/// Ingest a raw `tokscale --group-by session,model --json` value.
pub fn ingest_sessions(
    conn: &mut Connection,
    raw: &serde_json::Value,
) -> Result<usize, StorageError> {
    let r: SessionsReport = serde_json::from_value(raw.clone())?;
    let rows = rows_from_report(&r);
    upsert_rows(conn, &rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::schema;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    fn sample_report() -> serde_json::Value {
        // Trimmed from a real `tokscale --json --group-by session,model` run.
        serde_json::json!({
            "groupBy": "session,model",
            "entries": [
                {
                    "client": "claude",
                    "sessionId": "45ea27f1",
                    "model": "glm-5.2",
                    "provider": "unknown",
                    "input": 900, "output": 90, "cacheRead": 500, "cacheWrite": 0, "reasoning": 0,
                    "messageCount": 12, "cost": 1.11
                },
                {
                    "client": "claude",
                    "sessionId": "45ea27f1",
                    "model": "auto",
                    "provider": "unknown",
                    "input": 50, "output": 5, "cacheRead": 20, "cacheWrite": 0, "reasoning": 0,
                    "messageCount": 2, "cost": 0.10
                },
                {
                    "client": "codex",
                    "sessionId": "sess-x1",
                    "model": "gpt-5-plus",
                    "provider": "openai",
                    "input": 200, "output": 80, "cacheRead": 0, "cacheWrite": 0, "reasoning": 999,
                    "messageCount": 3, "cost": 0.42
                }
            ]
        })
    }

    #[test]
    fn rows_from_report_flattens_entries() {
        let r: SessionsReport = serde_json::from_value(sample_report()).unwrap();
        let rows = rows_from_report(&r);
        assert_eq!(rows.len(), 3);
        let claude = rows
            .iter()
            .find(|r| r.tool == "claude" && r.model == "glm-5.2")
            .unwrap();
        assert_eq!(claude.session_id, "45ea27f1");
        assert_eq!(claude.input, 900);
        assert!((claude.cost_usd - 1.11).abs() < 1e-9);
    }

    #[test]
    fn same_session_id_different_models_are_separate_rows() {
        // Composite PK (tool, session_id, model): two claude/45ea27f1 rows must coexist.
        let mut conn = fresh_conn();
        let n = ingest_sessions(&mut conn, &sample_report()).unwrap();
        assert_eq!(n, 3);
        let count_45ea: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE tool='claude' AND session_id='45ea27f1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count_45ea, 2);
    }

    #[test]
    fn upsert_replaces_absolute_totals() {
        let mut conn = fresh_conn();
        ingest_sessions(&mut conn, &sample_report()).unwrap();

        // Later report: same PK, larger absolute totals for claude/45ea27f1/glm-5.2.
        let later = serde_json::json!({
            "groupBy": "session,model",
            "entries": [{
                "client": "claude", "sessionId": "45ea27f1", "model": "glm-5.2",
                "input": 1200, "output": 130, "cacheRead": 800, "cacheWrite": 0,
                "cost": 1.66
            }]
        });
        ingest_sessions(&mut conn, &later).unwrap();

        let (inp, cost): (i64, f64) = conn
            .query_row(
                "SELECT input_tokens, cost_usd FROM sessions
                 WHERE tool='claude' AND session_id='45ea27f1' AND model='glm-5.2'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(inp, 1200);
        assert!((cost - 1.66).abs() < 1e-9);

        // and no duplicate row appeared
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 3);
    }

    #[test]
    fn empty_report_is_noop() {
        let mut conn = fresh_conn();
        let n = ingest_sessions(&mut conn, &serde_json::json!({"entries": []})).unwrap();
        assert_eq!(n, 0);
    }
}
