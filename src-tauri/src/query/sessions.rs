//! Session list (T2.3). V1 returns sessions ordered by token use; per-session
//! `started_at` / `last_used_at` / `project_path` are NULL until a richer
//! ingest lands (see storage/sessions.rs), so those fields stay out of the V1 VM.

use rusqlite::Connection;

use super::QueryError;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SessionVm {
    pub tool: String,
    pub session_id: String,
    pub model: String,
    pub tokens: i64,
    pub cost_usd: f64,
}

/// All sessions, ordered by tokens desc (V1 default sort).
pub fn query(conn: &Connection) -> Result<Vec<SessionVm>, QueryError> {
    let mut stmt = conn.prepare(
        "SELECT tool, session_id, model,
                input_tokens + output_tokens + cache_read_tokens + cache_write_tokens AS tokens,
                cost_usd
         FROM sessions
         ORDER BY tokens DESC",
    )?;
    let out = stmt
        .query_map([], |r| {
            Ok(SessionVm {
                tool: r.get::<_, String>(0)?,
                session_id: r.get::<_, String>(1)?,
                model: r.get::<_, String>(2)?,
                tokens: r.get::<_, i64>(3)?,
                cost_usd: r.get::<_, f64>(4)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{schema, sessions::ingest_sessions};

    fn seeded() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        ingest_sessions(
            &mut conn,
            &serde_json::json!({
                "entries": [
                    { "client": "claude", "sessionId": "s1", "model": "glm-5.2",
                      "input": 100, "output": 0, "cacheRead": 0, "cacheWrite": 0, "cost": 1.0 },
                    { "client": "codex", "sessionId": "s2", "model": "gpt-5",
                      "input": 500, "output": 500, "cacheRead": 0, "cacheWrite": 0, "cost": 5.0 }
                ]
            }),
        )
        .unwrap();
        conn
    }

    #[test]
    fn lists_sessions_ordered_by_tokens_desc() {
        let conn = seeded();
        let v = query(&conn).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].session_id, "s2"); // 1000 > 100
        assert_eq!(v[0].tokens, 1000);
        assert!((v[0].cost_usd - 5.0).abs() < 1e-9);
        assert_eq!(v[1].session_id, "s1");
    }

    #[test]
    fn empty_when_no_sessions() {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        assert!(query(&conn).unwrap().is_empty());
    }
}
