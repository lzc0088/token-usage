//! Project snapshots precomputed during collection (P0-阶段3 complete form).
//!
//! token-monitor's model: the collector precomputes everything, the UI reads
//! precomputed data. We mirror that here — on each history tick the collector
//! builds period-correct project lists (today / month / all) and persists them
//! to `app_config` under `projects_snapshot:{period}`. `get_projects` then
//! becomes a pure DB read (~ms), with a live-tokscale fallback for the very
//! first run before any tick has fired.
//!
//! Why 3 snapshots: tokscale's `--group-by` has no date×project dimension
//! (only period-bucketed session/model or workspace/model), so period-correct
//! project data requires separate today/month/all calls.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::tokscale::{self, Period};
use super::workspace::{
    build_projects_from_sessions_with_map, filter_out_client, is_visible_project, merge_project,
    parse_workspace_report, ProjectAgg,
};
use crate::config;

/// Snapshot key for a given period.
fn snapshot_key(period: &str) -> String {
    format!("projects_snapshot:{period}")
}

/// Build the session→project map from the `sessions` table (project_path is
/// populated by the runtime backfill). Map: session_id → (name, path, date).
fn session_project_map(conn: &Connection) -> HashMap<String, (String, String, Option<String>)> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT session_id, project_path, MAX(last_used_at) AS last_used
         FROM sessions
         WHERE tool = 'claude'
           AND project_path IS NOT NULL AND project_path != ''
         GROUP BY session_id, project_path",
    ) else {
        return HashMap::new();
    };
    let rows = stmt.query_map([], |r| {
        let sid: String = r.get(0)?;
        let path: String = r.get(1)?;
        let name = path
            .split('/')
            .rfind(|s| !s.is_empty())
            .unwrap_or(&path)
            .to_string();
        let date = r.get::<_, Option<i64>>(2)?.and_then(|ts| {
            Some(
                chrono::DateTime::from_timestamp_millis(ts)?
                    .format("%Y-%m-%d")
                    .to_string(),
            )
        });
        Ok((sid, (name, path, date)))
    });
    let Ok(rows) = rows else {
        return HashMap::new();
    };
    rows.filter_map(Result::ok).collect()
}

/// Run the 4 tokscale reports in parallel and build one project list for a
/// single period. `tp` selects the period (Today/Month/All).
async fn build_period_projects(
    bin: &Path,
    tp: Period,
    session_map: &HashMap<String, (String, String, Option<String>)>,
) -> Vec<ProjectAgg> {
    let sess_args = tokscale::report_args(tp, &["claude".to_string()], "session,model");
    let ws_args = tokscale::report_args(tp, &[], "workspace,model");
    let (sess_res, ws_res) = tokio::join!(
        tokscale::run_json(bin, &sess_args),
        tokscale::run_json(bin, &ws_args),
    );

    // Claude projects from session report + DB path map.
    let mut projects = match sess_res {
        Ok(json) if !session_map.is_empty() => {
            build_projects_from_sessions_with_map(&json, session_map)
        }
        _ => Vec::new(),
    };

    // Non-Claude tool projects from workspace report.
    if let Ok(ws_json) = ws_res {
        let ws_non_claude = filter_out_client(&ws_json, "claude");
        for ws in parse_workspace_report(&ws_non_claude, None) {
            merge_project(&mut projects, ws);
        }
    }

    projects.sort_by_key(|p| std::cmp::Reverse(p.tokens));
    // Filter out noise projects (stray one-off sessions). Cost alone must not
    // gate visibility — unpriced models would blank the page (see
    // is_visible_project).
    projects.retain(is_visible_project);
    projects
}

/// Precompute and persist project snapshots. Uses an anchor to avoid
/// rebuilding month/all on every tick — only today is rebuilt per tick;
/// month/all rebuild only on the hourly full scan or when the config changes.
///
/// This mirrors token-monitor's anchor/delta: a warm tick costs 1 tokscale pair
/// (~2s) instead of 3 (~6s), saving ~4s on 3 out of 4 ticks.
pub async fn precompute_and_persist(bin: PathBuf, db: Arc<Mutex<Connection>>) {
    let (session_map, needs_full) = {
        let Ok(conn) = db.lock() else {
            return;
        };
        let map = session_project_map(&conn);
        if map.is_empty() {
            return; // Backfill hasn't run yet — nothing to precompute.
        }

        // Check anchor: should we rebuild all 3 periods or just today?
        let needs = anchor_needs_full_rebuild(&conn);
        (map, needs)
    };

    let today = build_period_projects(&bin, Period::Today, &session_map).await;

    if needs_full {
        // Full rebuild (~6s): today + month + all.
        let month = build_period_projects(&bin, Period::Month, &session_map).await;
        let all = build_period_projects(&bin, Period::All, &session_map).await;
        if let Ok(conn) = db.lock() {
            let _ = config::set_json(&conn, &snapshot_key("today"), &today);
            let _ = config::set_json(&conn, &snapshot_key("month"), &month);
            let _ = config::set_json(&conn, &snapshot_key("total"), &all);
            let _ = save_anchor(&conn);
            tracing::debug!(
                "project snapshots (full): today={}, month={}, total={}",
                today.len(),
                month.len(),
                all.len()
            );
        }
    } else {
        // Warm tick (~2s): only today. Month/all stay as-is from last full scan.
        if let Ok(conn) = db.lock() {
            let _ = config::set_json(&conn, &snapshot_key("today"), &today);
            tracing::debug!("project snapshots (warm): today={}", today.len());
        }
    }
}

/// How often a full project rebuild is required (1 hour). Between these,
/// warm ticks only refresh today. This matches token-monitor's
/// FULL_SCAN_INTERVAL_MS pattern.
const FULL_REBUILD_INTERVAL_SECS: i64 = 3600;

/// Check whether we need a full rebuild (all 3 periods) or just today.
/// Full rebuild when:
/// - No anchor exists (first run)
/// - More than 1 hour since last full scan
/// - Config fingerprint changed (tracked/visible tool list changed)
fn anchor_needs_full_rebuild(conn: &Connection) -> bool {
    let Ok(Some(anchor)) = config::get_json::<SnapshotAnchor>(conn, "projects_snapshot_anchor")
    else {
        return true; // No anchor → full rebuild
    };
    let now = chrono::Utc::now().timestamp();
    if anchor.full_scan_at + FULL_REBUILD_INTERVAL_SECS < now {
        return true;
    }
    // Config changed?
    if anchor.config_fingerprint != snapshot_config_fingerprint(conn) {
        return true;
    }
    false
}

fn save_anchor(conn: &Connection) -> Result<(), String> {
    let anchor = SnapshotAnchor {
        full_scan_at: chrono::Utc::now().timestamp(),
        config_fingerprint: snapshot_config_fingerprint(conn),
    };
    config::set_json(conn, "projects_snapshot_anchor", &anchor).map_err(|e| e.to_string())
}

/// Compute a fingerprint from collection config to detect changes.
fn snapshot_config_fingerprint(conn: &Connection) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for key in &["collection_tracked", "collection_visible"] {
        if let Ok(Some(val)) = config::get_raw(conn, key) {
            h.write(val.as_bytes());
        }
        h.write_u8(0);
    }
    h.finish()
}

/// A persisted anchor recording when the last full project rebuild ran and
/// under what config, so warm ticks can skip expensive month/all scans.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SnapshotAnchor {
    full_scan_at: i64,
    config_fingerprint: u64,
}

/// Load a precomputed project snapshot for the given period. Returns None if
/// no snapshot has been built yet (first run) or it's corrupted.
/// Filters out any stale internal Claude paths (e.g. ~/.claude-mem/observer-sessions)
/// that may have been captured before the decode_workspace filter was added.
pub fn load_snapshot(conn: &Connection, period: &str) -> Option<Vec<ProjectAgg>> {
    let mut projects: Vec<ProjectAgg> = config::get_json(conn, &snapshot_key(period))
        .ok()
        .flatten()?;
    projects.retain(|p| {
        !p.full_path.as_deref().is_some_and(|fp| {
            fp.contains(".claude-mem") || fp.contains("/observer-") || fp.contains("\\observer-")
        })
    });
    Some(projects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_key_is_period_scoped() {
        assert_eq!(snapshot_key("day"), "projects_snapshot:day");
        assert_eq!(snapshot_key("month"), "projects_snapshot:month");
        assert_eq!(snapshot_key("total"), "projects_snapshot:total");
    }

    #[test]
    fn load_returns_none_when_absent() {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        assert!(load_snapshot(&conn, "today").is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        let projects = vec![ProjectAgg {
            name: "demo".into(),
            full_path: Some("~/demo".into()),
            latest_date: Some("2026-07-30".into()),
            tokens: 1000,
            cost_usd: 1.5,
            messages: 10,
            models: Vec::new(),
            tools: Vec::new(),
        }];
        config::set_json(&conn, &snapshot_key("today"), &projects).unwrap();
        let loaded = load_snapshot(&conn, "today").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "demo");
        assert_eq!(loaded[0].tokens, 1000);
    }

    #[test]
    fn session_project_map_empty_when_no_rows() {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        assert!(session_project_map(&conn).is_empty());
    }

    #[test]
    fn session_project_map_reads_project_path() {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions (tool, session_id, model, project_path, last_used_at, message_count)
             VALUES ('claude', 'sess-1', 'glm-5.2', '/Users/z/demo', 1753872000000, 5)",
            [],
        )
        .unwrap();
        let map = session_project_map(&conn);
        assert_eq!(map.len(), 1);
        let (name, path, date) = &map["sess-1"];
        assert_eq!(name, "demo");
        assert_eq!(path, "/Users/z/demo");
        assert!(date.is_some());
    }
}
