//! Project statistics (T2.3). `sessions.project_path` is NULL in V1 (tokscale's
//! report shape doesn't surface it per-session), so this returns an empty list
//! until a richer ingest populates `project_path`. The query itself is correct
//! and ready for when the data lands.

use rusqlite::Connection;

use super::QueryError;

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectVm {
    pub path: String,
    pub tokens: i64,
    pub cost_usd: f64,
    pub session_count: i64,
}

/// Group sessions by `project_path`, excluding NULL paths. Ordered by tokens desc.
pub fn query(conn: &Connection) -> Result<Vec<ProjectVm>, QueryError> {
    let mut stmt = conn.prepare(
        "SELECT project_path,
                COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens + cache_write_tokens), 0) AS tokens,
                COALESCE(SUM(cost_usd), 0),
                COUNT(*)
         FROM sessions
         WHERE project_path IS NOT NULL AND project_path <> ''
         GROUP BY project_path
         ORDER BY tokens DESC",
    )?;
    let out = stmt
        .query_map([], |r| {
            Ok(ProjectVm {
                path: r.get::<_, String>(0)?,
                tokens: r.get::<_, i64>(1)?,
                cost_usd: r.get::<_, f64>(2)?,
                session_count: r.get::<_, i64>(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::schema;

    #[test]
    fn empty_when_no_project_paths() {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        // rows with NULL project_path are excluded
        conn.execute(
            "INSERT INTO sessions (tool, session_id, model, input_tokens, output_tokens,
                                   cache_read_tokens, cache_write_tokens, cost_usd)
             VALUES ('claude','s1','glm-5.2',100,0,0,0,1.0)",
            [],
        )
        .unwrap();
        assert!(query(&conn).unwrap().is_empty());
    }

    #[test]
    fn groups_by_project_path() {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions (tool, session_id, model, input_tokens, output_tokens,
                                   cache_read_tokens, cache_write_tokens, cost_usd, project_path)
             VALUES
               ('claude','s1','glm-5.2', 1000,0,0,0, 1.0, '/work/app-a'),
               ('claude','s2','glm-5.2',  500,0,0,0, 0.5, '/work/app-a'),
               ('codex', 's3','gpt-5',    200,0,0,0, 0.2, '/work/app-b')",
            [],
        )
        .unwrap();
        let v = query(&conn).unwrap();
        assert_eq!(v.len(), 2);
        // app-a aggregates two sessions → 1500 tokens, ordered first
        assert_eq!(v[0].path, "/work/app-a");
        assert_eq!(v[0].tokens, 1500);
        assert!((v[0].cost_usd - 1.5).abs() < 1e-9);
        assert_eq!(v[0].session_count, 2);
        assert_eq!(v[1].path, "/work/app-b");
        assert_eq!(v[1].session_count, 1);
    }
}
