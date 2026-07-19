//! Read-only query commands (M3 T3.1). All sync, all read from `daily_usage` /
//! `sessions` via the `query` layer.

use tauri::State;

use crate::commands::{db, parse_period, today};
use crate::query::{
    self, breakdown::Breakdown, projects::ProjectVm, sessions::SessionVm, trends::Trends, Dimension,
};
use crate::state::AppState;

/// Totals for the popover hero: input/output/cache totals + cost + message count.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Summary {
    pub period: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
    pub messages: i64,
}

#[tauri::command]
pub fn get_summary(period: String, state: State<AppState>) -> Result<Summary, String> {
    let p = parse_period(&period);
    let range = query::range_for_period(p, &today());
    let conn = db(&state);
    let (clause, params) = query::range_clause(&range);
    let sql = format!(
        "SELECT
            COALESCE(SUM(input_tokens),0),
            COALESCE(SUM(output_tokens),0),
            COALESCE(SUM(cache_read_tokens),0),
            COALESCE(SUM(cache_write_tokens),0),
            COALESCE(SUM(reasoning_tokens),0),
            COALESCE(SUM(cost_usd),0),
            COALESCE(SUM(messages),0)
         FROM daily_usage WHERE {clause}"
    );
    let row = conn
        .query_row(&sql, rusqlite::params_from_iter(params), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let (input, output, cache_read, cache_write, reasoning, cost, messages) = row;
    Ok(Summary {
        period,
        input,
        output,
        cache_read,
        cache_write,
        reasoning,
        total_tokens: input + output + cache_read + cache_write, // reasoning excluded
        cost_usd: cost,
        messages,
    })
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
pub fn get_trends(period: String, state: State<AppState>) -> Result<Trends, String> {
    let p = parse_period(&period);
    let range = query::range_for_period(p, &today());
    query::trends::query(&db(&state), &range).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_sessions(state: State<AppState>) -> Result<Vec<SessionVm>, String> {
    query::sessions::query(&db(&state)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_projects(state: State<AppState>) -> Result<Vec<ProjectVm>, String> {
    query::projects::query(&db(&state)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_period_maps_strings() {
        assert_eq!(parse_period("day"), query::Period::Day);
        assert_eq!(parse_period("month"), query::Period::Month);
        assert_eq!(parse_period("total"), query::Period::Total);
        assert_eq!(parse_period("garbage"), query::Period::Total); // default
    }

    #[test]
    fn today_is_iso_date() {
        let t = today();
        assert_eq!(t.len(), 10);
        assert_eq!(t.chars().nth(4), Some('-'));
    }
}
