//! Export commands — JSON + CSV serialization of usage data.

use serde::Serialize;
use tauri::State;

use crate::commands::{db, parse_period, today};
use crate::query::{self, breakdown::Breakdown, sessions::SessionVm, trends::Trends, Dimension};
use crate::state::AppState;

#[derive(Serialize)]
struct ExportSnapshot {
    period: String,
    summary: query::summary::Summary,
    breakdown_by_tool: Breakdown,
    breakdown_by_model: Breakdown,
}

#[derive(Serialize)]
struct ExportPayload {
    generated_at: String,
    app: serde_json::Value,
    snapshots: Vec<ExportSnapshot>,
    daily_trends: Trends,
    sessions: Vec<SessionVm>,
}

#[tauri::command]
pub fn export_json(state: State<AppState>) -> Result<String, String> {
    let conn = db(&state);
    let today = today();

    let periods = ["day", "month", "total"];
    let mut snapshots = Vec::new();

    for period_str in &periods {
        let p = parse_period(period_str);
        let range = query::range_for_period(p, &today);
        let summary = query::summary::query(&conn, &range).map_err(|e| e.to_string())?;
        let tool_breakdown =
            query::breakdown::query(&conn, &range, Dimension::Tool).map_err(|e| e.to_string())?;
        let model_breakdown =
            query::breakdown::query(&conn, &range, Dimension::Model).map_err(|e| e.to_string())?;

        snapshots.push(ExportSnapshot {
            period: period_str.to_string(),
            summary,
            breakdown_by_tool: tool_breakdown,
            breakdown_by_model: model_breakdown,
        });
    }

    let total_range = query::range_for_period(query::Period::Total, &today);
    let daily_trends = query::trends::query(&conn, &total_range).map_err(|e| e.to_string())?;

    // Get Claude projects dir for session queries
    let claude_projects_dir = dirs::home_dir().map(|h| h.join(".claude").join("projects"));
    let sessions = query::sessions::query(&conn, claude_projects_dir.as_deref(), Some(500))
        .map_err(|e| e.to_string())?;

    let payload = ExportPayload {
        generated_at: chrono::Utc::now().to_rfc3339(),
        app: serde_json::json!({
            "name": "token-usage",
            "version": env!("CARGO_PKG_VERSION"),
        }),
        snapshots,
        daily_trends,
        sessions,
    };

    serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_csv(state: State<AppState>) -> Result<String, String> {
    let conn = db(&state);
    let today = today();

    let periods = ["day", "month", "total"];
    let mut rows: Vec<Vec<String>> = Vec::new();
    rows.push(vec![
        "period".into(),
        "dimension".into(),
        "name".into(),
        "tokens".into(),
        "cost_usd".into(),
        "messages".into(),
        "input".into(),
        "output".into(),
        "cache_read".into(),
        "cache_write".into(),
    ]);

    for period_str in &periods {
        let p = parse_period(period_str);
        let range = query::range_for_period(p, &today);

        for (dim_label, dim) in [("tool", Dimension::Tool), ("model", Dimension::Model)] {
            let breakdown =
                query::breakdown::query(&conn, &range, dim).map_err(|e| e.to_string())?;
            for entry in &breakdown.entries {
                rows.push(vec![
                    period_str.to_string(),
                    dim_label.to_string(),
                    csv_escape(&entry.key),
                    entry.tokens.to_string(),
                    format!("{:.6}", entry.cost_usd),
                    entry.messages.to_string(),
                    entry.input.to_string(),
                    entry.output.to_string(),
                    entry.cache_read.to_string(),
                    entry.cache_write.to_string(),
                ]);
            }
        }
    }

    Ok(rows
        .iter()
        .map(|r| r.join(","))
        .collect::<Vec<_>>()
        .join("\r\n"))
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[tauri::command]
pub fn copy_to_clipboard(text: String) -> Result<(), String> {
    // Use arboard for clipboard access
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(())
}
