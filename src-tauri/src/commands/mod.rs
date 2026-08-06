//! Tauri `#[command]` interface (M3 T3.1). Thin wrappers over the query /
//! config / collector layers; the heavy logic lives behind them and is unit
//! tested there. Commands only (de)serialize + borrow [`AppState`].

pub(crate) mod autostart;
pub(crate) mod collection;
pub(crate) mod copilot;
pub(crate) mod exchange;
pub(crate) mod query;
pub(crate) mod quota;
pub(crate) mod settings;
pub(crate) mod status;
pub(crate) mod update;
pub(crate) mod window_cmd;

use tauri::State;

use crate::state::AppState;

/// Today's date as `YYYY-MM-DD` (local). Used to turn the global period into a
/// concrete `daily_usage` range.
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Map the frontend period string ("day" | "month" | "total") to a query Period.
fn parse_period(s: &str) -> crate::query::Period {
    match s {
        "day" => crate::query::Period::Day,
        "month" => crate::query::Period::Month,
        _ => crate::query::Period::Total,
    }
}

/// Lock the DB for reading (shared). Multiple concurrent readers are allowed
/// — SQLite WAL mode supports this. Commands are sync; the guard is held only
/// across the (fast) synchronous query, never across an `.await`.
pub(crate) fn db<'r>(
    state: &'r State<AppState>,
) -> std::sync::MutexGuard<'r, rusqlite::Connection> {
    state.db_read()
}

/// Lock the DB for writing (expressed intent — guard type is `MutexGuard`,
/// same as read, but callers may mutate through this guard). Use for inserts,
/// updates, deletes, and transactions.
pub(crate) fn db_write<'r>(
    state: &'r State<AppState>,
) -> std::sync::MutexGuard<'r, rusqlite::Connection> {
    state.db_write()
}
