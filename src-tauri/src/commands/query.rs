//! Read-only query commands (M3 T3.1/T3.3). Sync; read from `daily_usage` /
//! `sessions` via the `query` layer.

use tauri::State;

use std::collections::HashSet;

use crate::collector::workspace::{
    build_projects_from_sessions_with_map, filter_out_client, merge_project,
    parse_workspace_report, scan_claude_filesystem, ProjectAgg,
};
use crate::commands::{db, parse_period, today};
use crate::query::summary::Summary;
use crate::query::{self, breakdown::Breakdown, sessions::SessionVm, trends::Trends, Dimension};
use crate::state::AppState;

#[tauri::command]
pub fn get_summary(period: String, state: State<AppState>) -> Result<Summary, String> {
    let p = parse_period(&period);
    let t = today();
    let range = query::range_for_period(p, &t);
    let mut s = query::summary::query(&db(&state), &range).map_err(|e| e.to_string())?;

    // Compute delta vs previous period (e.g. 较昨日 / 较上月).
    if let Some((prev_range, label)) = query::prev_range_for_period(p, &t) {
        if let Ok(prev) = query::summary::query(&db(&state), &prev_range) {
            if prev.total_tokens > 0 {
                let delta =
                    (s.total_tokens - prev.total_tokens) as f64 / prev.total_tokens as f64 * 100.0;
                s.delta_pct = Some(delta);
                s.delta_label = Some(label.to_string());
            }
        }
    }

    Ok(s)
}

#[tauri::command]
pub fn get_breakdown(
    period: String,
    dimension: String,
    state: State<AppState>,
) -> Result<Breakdown, String> {
    let p = parse_period(&period);
    let dim = match dimension.as_str() {
        "model" => Dimension::Model,
        _ => Dimension::Tool,
    };
    let range = query::range_for_period(p, &today());
    query::breakdown::query(&db(&state), &range, dim).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_detail_breakdown(
    period: String,
    dimension: String,
    filter: String,
    state: State<AppState>,
) -> Result<Breakdown, String> {
    let p = parse_period(&period);
    let dim = match dimension.as_str() {
        "model" => Dimension::Model,
        _ => Dimension::Tool,
    };
    let range = query::range_for_period(p, &today());
    query::breakdown::query_filtered(&db(&state), &range, dim, &filter).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_trends(period: String, state: State<AppState>) -> Result<Trends, String> {
    let p = parse_period(&period);
    let today = today();
    let conn = db(&state);
    // DAY → last 7 days (daily); MONTH → current month (daily);
    // TOTAL → all history grouped by month.
    let res = match p {
        query::Period::Day => {
            let range = query::last_n_days(&today, 7);
            query::trends::query(&conn, &range)
        }
        query::Period::Month => {
            let range = query::range_for_period(query::Period::Month, &today);
            query::trends::query(&conn, &range)
        }
        query::Period::Total => query::trends::query_monthly(&conn, &query::DateRange::default()),
    };
    res.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_sessions(state: State<AppState>) -> Result<Vec<SessionVm>, String> {
    let claude_dir = dirs::home_dir().map(|h| h.join(".claude").join("projects"));
    query::sessions::query(&db(&state), claude_dir.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_session_detail(
    tool: String,
    session_id: String,
    state: State<AppState>,
) -> Result<Vec<query::sessions::SessionDetailRow>, String> {
    query::sessions::query_detail(&db(&state), &tool, &session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_session_rounds(
    tool: String,
    session_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<query::sessions::SessionRoundVm>, String> {
    let home_dir = dirs::home_dir();
    // Snapshot the DB rows we need under a short-lived lock, then release it
    // before the (potentially slow) file parse runs on the blocking pool.
    let model_totals = {
        let conn = db(&state);
        query::sessions::session_model_totals_public(&conn, &tool, &session_id)
            .map_err(|e| e.to_string())?
    };
    let tool_cl = tool.clone();
    let sid_cl = session_id.clone();
    let home = home_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        query::sessions::build_rounds(home.as_deref(), &tool_cl, &sid_cl, model_totals)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Build the project list by merging two tokscale reports:
///
/// 1. `--group-by session,model` → for **Claude** sessions, each session is
///    mapped to its JSONL `cwd` so subdirectories like
///    `bee_miniprogram/uniapp-field` appear as independent projects.
/// 2. `--group-by workspace,model` → for **non-Claude** tools (codex, zcode,
///    workbuddy, etc.), the workspaceKey is already the real project path.
///
/// A filesystem scan of `~/.claude/projects/` supplements projects with zero
/// activity in the queried period.
#[tauri::command]
pub async fn get_projects(
    app: tauri::AppHandle,
    period: String,
) -> Result<Vec<ProjectAgg>, String> {
    let p = parse_period(&period);
    let tp = match p {
        query::Period::Day => crate::collector::tokscale::Period::Today,
        query::Period::Month => crate::collector::tokscale::Period::Month,
        query::Period::Total => crate::collector::tokscale::Period::All,
    };

    let data = match crate::collector::tokscale::app_bin_dir() {
        Some(d) => d,
        None => return Ok(Vec::new()),
    };
    let custom = crate::collector::tokscale::bundled_bin_path(&app);
    let bin = match crate::collector::tokscale::resolve_bin(custom.as_deref(), &data) {
        Ok(b) => b,
        Err(_) => return Ok(Vec::new()),
    };

    let claude_dir = dirs::home_dir().map(|h| h.join(".claude").join("projects"));

    // ── Two tokscale reports, run in parallel ───────────────────────────
    // Report 1 (session,model, -c claude): Claude per-session data — small,
    // fast, and precise enough to map each session to its JSONL cwd.
    // Report 2 (workspace,model): non-Claude tools whose workspaceKey is a
    // real path. Claude entries are filtered out in memory afterwards.
    let sess_args =
        crate::collector::tokscale::report_args(tp, &["claude".to_string()], "session,model");
    let ws_args = crate::collector::tokscale::report_args(tp, &[], "workspace,model");
    let (sess_res, ws_res) = tokio::join!(
        crate::collector::tokscale::run_json(&bin, &sess_args),
        crate::collector::tokscale::run_json(&bin, &ws_args),
    );
    let sess_json = sess_res.map_err(|e| e.to_string())?;
    let ws_json = ws_res.map_err(|e| e.to_string())?;

    // ── Filesystem walk + aggregation on the blocking pool ──────────────
    // scan_claude_filesystem walks ~/.claude/projects/ once (capped reads),
    // yielding both the session→project map and the full project list —
    // shared by the session grouping and the zero-activity supplement.
    let projects = tauri::async_runtime::spawn_blocking(move || -> Vec<ProjectAgg> {
        let empty: std::collections::HashMap<String, (String, String, Option<String>)> =
            std::collections::HashMap::new();
        let fs = claude_dir.as_deref().map(scan_claude_filesystem);
        let session_map = fs.as_ref().map(|f| &f.session_map).unwrap_or(&empty);

        let mut projects = build_projects_from_sessions_with_map(&sess_json, session_map);

        // Non-Claude tool projects (workspace-level report, Claude filtered out).
        let ws_non_claude = filter_out_client(&ws_json, "claude");
        for ws in parse_workspace_report(&ws_non_claude, None) {
            merge_project(&mut projects, ws);
        }

        // Zero-activity Claude projects (from the single filesystem scan).
        if let Some(fs) = &fs {
            let existing: HashSet<Option<String>> =
                projects.iter().map(|p| p.full_path.clone()).collect();
            for dw in &fs.all_projects {
                if !existing.contains(&dw.full_path) {
                    projects.push(ProjectAgg {
                        name: dw.name.clone(),
                        full_path: dw.full_path.clone(),
                        latest_date: dw.latest_date.clone(),
                        tokens: 0,
                        cost_usd: 0.0,
                        messages: 0,
                        models: Vec::new(),
                        tools: Vec::new(),
                    });
                }
            }
        }

        projects.sort_by_key(|p| std::cmp::Reverse(p.tokens));

        // Filter out projects that would be invisible to the user — matches
        // the frontend filter so we don't transfer useless data over IPC.
        projects.retain(|p| {
            (p.full_path.is_some() || p.latest_date.is_some())
                && p.messages >= 5
                && p.cost_usd >= 0.1
        });

        projects
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(projects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_period_maps_strings() {
        assert_eq!(parse_period("day"), query::Period::Day);
        assert_eq!(parse_period("month"), query::Period::Month);
        assert_eq!(parse_period("total"), query::Period::Total);
        assert_eq!(parse_period("garbage"), query::Period::Total);
    }

    #[test]
    fn today_is_iso_date() {
        let t = today();
        assert_eq!(t.len(), 10);
        assert_eq!(t.chars().nth(4), Some('-'));
    }
}
