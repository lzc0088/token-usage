//! Read-only query commands (M3 T3.1/T3.3). Sync; read from `daily_usage` /
//! `sessions` via the `query` layer.

use tauri::State;

use crate::collector::workspace::{build_projects_from_sessions, ProjectAgg};
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

/// Build the project list from tokscale's `--group-by session,model` report.
///
/// Unlike the old workspace-key approach, this reads each Claude session's
/// JSONL `cwd` field to determine the project — so subdirectories like
/// `bee_miniprogram/uniapp-field` appear as independent projects rather than
/// being merged into `bee_miniprogram`. Token/cost data stays precise
/// (from tokscale), project names come from the JSONL files (authoritative).
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

    // Query session-level (NOT workspace-level) data so we can re-group by
    // per-session cwd rather than by tokscale's workspaceKey.
    let args = crate::collector::tokscale::report_args(tp, &[], "session,model");
    let json = crate::collector::tokscale::run_json(&bin, &args)
        .await
        .map_err(|e| e.to_string())?;

    let claude_dir = dirs::home_dir().map(|h| h.join(".claude").join("projects"));
    Ok(build_projects_from_sessions(&json, claude_dir.as_deref()))
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
