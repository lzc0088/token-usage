//! Read-only query commands (M3 T3.1/T3.3). Sync; read from `daily_usage` /
//! `sessions` via the `query` layer.

use tauri::State;

use crate::commands::{db, parse_period, today};
use crate::query::summary::Summary;
use crate::query::{
    self, breakdown::Breakdown, projects::ProjectVm, sessions::SessionVm, trends::Trends, Dimension,
};
use crate::state::AppState;

#[tauri::command]
pub fn get_summary(period: String, state: State<AppState>) -> Result<Summary, String> {
    let p = parse_period(&period);
    let range = query::range_for_period(p, &today());
    query::summary::query(&db(&state), &range).map_err(|e| e.to_string())
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
        assert_eq!(parse_period("garbage"), query::Period::Total);
    }

    #[test]
    fn today_is_iso_date() {
        let t = today();
        assert_eq!(t.len(), 10);
        assert_eq!(t.chars().nth(4), Some('-'));
    }
}
