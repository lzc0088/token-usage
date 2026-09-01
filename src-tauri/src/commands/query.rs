//! Read-only query commands (M3 T3.1/T3.3). Sync; read from `daily_usage` /
//! `sessions` via the `query` layer.

use tauri::State;
use tracing::warn;

use rusqlite::Connection;

use crate::collector::workspace::ProjectAgg;
use crate::commands::{db, parse_period, today};
use crate::query::summary::Summary;
use crate::query::DateRange;
use crate::query::{self, breakdown::Breakdown, sessions::SessionVm, trends::Trends, Dimension};
use crate::state::AppState;

#[tauri::command]
pub fn get_summary(period: String, state: State<AppState>) -> Result<Summary, String> {
    let p = parse_period(&period);
    let t0 = std::time::Instant::now();

    // For "day", prefer the cached LIVE today Summary (written by the
    // collector on every `tokscale --today` scan). This keeps the popover in
    // sync with the tray title — both show the same real-time value. The
    // DB-backed `daily_usage` query lags one history tick (~15 min) and would
    // make the popover appear stale vs. the tray.
    if matches!(p, query::Period::Day) {
        if let Ok(cache) = state.last_today.lock() {
            if let Some(ref live) = *cache {
                // Recompute the vs-yesterday delta on top of the live value
                // (the live Summary has delta_pct = None).
                let mut s = live.clone();
                let t = today();
                if let Some((prev_range, label)) = query::prev_range_for_period(p, &t) {
                    s.delta_pct = try_delta(&db(&state), s.total_tokens, &prev_range);
                    if s.delta_pct.is_some() {
                        s.delta_label = Some(label.to_string());
                    }
                }
                tracing::debug!(period = ?p, elapsed_ms = ?t0.elapsed().as_millis(), "get_summary: live cache hit");
                return Ok(s);
            }
        }
    }

    let t = today();
    let range = query::range_for_period(p, &t);
    let mut s = query::summary::query(&db(&state), &range).map_err(|e| e.to_string())?;

    // Compute delta vs previous period (e.g. 较昨日 / 较上月).
    if let Some((prev_range, label)) = query::prev_range_for_period(p, &t) {
        s.delta_pct = try_delta(&db(&state), s.total_tokens, &prev_range);
        if s.delta_pct.is_some() {
            s.delta_label = Some(label.to_string());
        }
    }

    tracing::debug!(period = ?p, elapsed_ms = ?t0.elapsed().as_millis(), tokens = s.total_tokens, "get_summary: db query ok");
    Ok(s)
}

/// Try to compute the delta percentage between a live total and the previous
/// period's total. Logs a warning on DB errors so debugging is possible.
fn try_delta(conn: &Connection, live_total: i64, prev_range: &DateRange) -> Option<f64> {
    match query::summary::query(conn, prev_range) {
        Ok(prev) if prev.total_tokens > 0 => {
            Some((live_total - prev.total_tokens) as f64 / prev.total_tokens as f64 * 100.0)
        }
        Ok(_) => None,
        Err(e) => {
            warn!(error = %e, "delta query failed");
            None
        }
    }
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
    tracing::info!(?period, "get_trends: entry");
    let p = parse_period(&period);
    let today = today();
    let conn = db(&state);
    let t0 = std::time::Instant::now();
    // DAY → last 7 days (daily); MONTH → current month (daily);
    // TOTAL → all history daily so the heatmap (Trend.svelte) gets proper
    // YYYY-MM-DD dates and enough granularity to render.
    let res = match p {
        query::Period::Day => {
            let range = query::last_n_days(&today, 7);
            tracing::debug!(?range, "get_trends: day range");
            query::trends::query(&conn, &range)
        }
        query::Period::Month => {
            let range = query::range_for_period(query::Period::Month, &today);
            tracing::debug!(?range, "get_trends: month range");
            query::trends::query(&conn, &range)
        }
        query::Period::Total => {
            tracing::debug!("get_trends: total daily (unbounded)");
            // DAILY on purpose: the activity heatmap needs YYYY-MM-DD dates
            // to place cells. The frontend aggregates to monthly buckets for
            // the line chart (see Trend.svelte chartPoints).
            query::trends::query(&conn, &query::DateRange::default())
        }
    };
    let elapsed = t0.elapsed();
    match &res {
        Ok(t) => {
            tracing::info!(points = t.points.len(), elapsed_ms = ?elapsed.as_millis(), "get_trends: success")
        }
        Err(e) => tracing::warn!(err = %e, elapsed_ms = ?elapsed.as_millis(), "get_trends: error"),
    }
    res.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_sessions(limit: Option<i64>, state: State<AppState>) -> Result<Vec<SessionVm>, String> {
    tracing::info!(?limit, "get_sessions: entry");
    let claude_dir = dirs::home_dir().map(|h| h.join(".claude").join("projects"));
    let res = query::sessions::query(&db(&state), claude_dir.as_deref(), limit)
        .map_err(|e| e.to_string());
    match &res {
        Ok(v) => tracing::info!(count = v.len(), "get_sessions: success"),
        Err(e) => tracing::warn!(err = %e, "get_sessions: error"),
    }
    res
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

/// Return the project list.
///
/// **Primary path (instant)**: read a precomputed snapshot from the DB. The
/// collector builds snapshots for today / month / total on each history tick
/// (~15 min) via `project_snapshot::precompute_and_persist`, so the common case
/// is a pure DB read — no tokscale call, no filesystem scan.
///
/// **Fallback path (~2s)**: if no snapshot exists yet (very first run before
/// any collection tick), fetch live from tokscale in parallel. The next tick
/// will build the snapshot, so subsequent opens are instant.
#[tauri::command]
pub async fn get_projects(
    app: tauri::AppHandle,
    period: String,
    state: State<'_, AppState>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<ProjectAgg>, String> {
    // ── 0. Snapshot read (pure DB, instant) ─────────────────────────────
    {
        let conn = db(&state);
        // Frontend period "day" maps to the "today" snapshot key.
        let snap_period = if period == "day" { "today" } else { &period };
        if let Some(projects) =
            crate::collector::project_snapshot::load_snapshot(&conn, snap_period)
        {
            tracing::debug!(
                "projects snapshot hit (period={period}, {})",
                projects.len()
            );
            return Ok(apply_pagination(projects, offset, limit));
        }
    } // conn guard dropped here

    // ── 1. Fallback: no snapshot yet → live tokscale ────────────────────
    let session_map: std::collections::HashMap<String, (String, String, Option<String>)> = {
        let conn = db(&state);
        let mut stmt = conn
            .prepare(
                "SELECT session_id, project_path, MAX(last_used_at) AS last_used
                 FROM sessions
                 WHERE tool = 'claude'
                   AND project_path IS NOT NULL AND project_path != ''
                 GROUP BY session_id, project_path",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
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
            })
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        rows.into_iter().collect()
    }; // conn guard dropped here

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

    let sess_args =
        crate::collector::tokscale::report_args(tp, &["claude".to_string()], "session,model");
    let ws_args = crate::collector::tokscale::report_args(tp, &[], "workspace,model");
    let bin_cl = bin.clone();
    let (sess_res, ws_res) = tokio::join!(
        crate::collector::tokscale::run_json(&bin, &sess_args),
        crate::collector::tokscale::run_json(&bin_cl, &ws_args),
    );

    let mut projects: Vec<ProjectAgg> = if session_map.is_empty() {
        Vec::new()
    } else {
        match sess_res {
            Ok(json) => crate::collector::workspace::build_projects_from_sessions_with_map(
                &json,
                &session_map,
            ),
            Err(e) => {
                tracing::warn!(error = %e, "tokscale session report failed");
                Vec::new()
            }
        }
    };
    if let Ok(ws_json) = ws_res {
        let ws_non_claude = crate::collector::workspace::filter_out_client(&ws_json, "claude");
        let ws_projects = crate::collector::workspace::parse_workspace_report(&ws_non_claude, None);
        for ws in ws_projects {
            crate::collector::workspace::merge_project(&mut projects, ws);
        }
    } else if projects.is_empty() {
        // Both sources failed — surface an error so the frontend can show a message.
        return Err("项目数据获取失败，请检查 tokscale 是否正常运行".into());
    }

    projects.sort_by_key(|x| std::cmp::Reverse(x.tokens));
    projects.retain(crate::collector::workspace::is_visible_project);

    Ok(apply_pagination(projects, offset, limit))
}

/// Apply offset/limit pagination to the project list.
fn apply_pagination(
    mut projects: Vec<ProjectAgg>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Vec<ProjectAgg> {
    let offset_val = offset.unwrap_or(0).max(0);
    let limit_val = limit.unwrap_or(100).clamp(1, 500);
    if offset_val > 0 {
        let start = (offset_val as usize).min(projects.len());
        projects = projects.split_off(start);
    }
    projects.truncate(limit_val as usize);
    projects
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
