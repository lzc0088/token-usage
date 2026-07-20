//! Read-only query commands (M3 T3.1/T3.3). Sync; read from `daily_usage` /
//! `sessions` via the `query` layer.

use std::collections::HashSet;

use tauri::State;

use crate::collector::workspace::{parse_workspace_report, ProjectAgg, scan_claude_projects};
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
    let range = query::range_for_period(p, &today());
    query::trends::query(&db(&state), &range).map_err(|e| e.to_string())
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
    let claude_dir = dirs::home_dir().map(|h| h.join(".claude").join("projects"));
    // Snapshot the DB rows we need under a short-lived lock, then release it
    // before the (potentially slow) file parse runs on the blocking pool.
    let model_totals = {
        let conn = db(&state);
        query::sessions::session_model_totals_public(&conn, &tool, &session_id)
            .map_err(|e| e.to_string())?
    };
    let tool_cl = tool.clone();
    let sid_cl = session_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        query::sessions::build_rounds(claude_dir.as_deref(), &tool_cl, &sid_cl, model_totals)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Per-project usage, sourced live from `tokscale report --group-by
/// workspace,model` with the global period applied. Projects aren't persisted
/// (the `daily_usage` table has no project dimension), so this is an on-demand
/// query — like a breakdown, not a DB read. Returns an empty list when tokscale
/// isn't available yet (e.g. still installing on first launch).
#[tauri::command]
pub async fn get_projects(period: String) -> Result<Vec<ProjectAgg>, String> {
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
    let bin = match crate::collector::tokscale::resolve_bin(None, &data) {
        Ok(b) => b,
        Err(_) => return Ok(Vec::new()),
    };

    let args = crate::collector::tokscale::report_args(tp, &[], "workspace,model");
    let json = crate::collector::tokscale::run_json(&bin, &args)
        .await
        .map_err(|e| e.to_string())?;

    let claude_dir = dirs::home_dir().map(|h| h.join(".claude").join("projects"));
    let mut projects = parse_workspace_report(&json, claude_dir.as_deref());

    // Supplement with filesystem-scan: include ALL Claude Code projects
    // that have session JSONL files, even if tokscale didn't report them
    // (no activity in the selected period). This matches token-monitor's
    // approach — always show every project, not just active ones.
    if let Some(ref dir) = claude_dir {
        let existing: HashSet<Option<String>> = projects.iter().map(|p| p.full_path.clone()).collect();
        for ws in scan_claude_projects(dir) {
            if !existing.contains(&ws.full_path) {
                projects.push(ProjectAgg {
                    name: ws.name,
                    full_path: ws.full_path.clone(),
                    latest_date: ws.latest_date,
                    tokens: 0,
                    cost_usd: 0.0,
                    messages: 0,
                    models: Vec::new(),
                    tools: Vec::new(),
                });
            }
        }
        // Re-sort so zero-activity projects sink to bottom
        projects.sort_by_key(|y| std::cmp::Reverse(y.tokens));
    }

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
