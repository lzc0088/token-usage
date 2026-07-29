//! Tauri `#[command]` interface (M3 T3.1). Thin wrappers over the query /
//! config / collector layers; the heavy logic lives behind them and is unit
//! tested there. Commands only (de)serialize + borrow [`AppState`].

pub(crate) mod autostart;
pub(crate) mod collection;
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

/// Lock the DB. Recovers from mutex poisoning (data is still valid after a
/// thread panic while holding the lock). Commands are sync; the lock is held
/// only across the (fast) synchronous query, never across an `.await`.
pub(crate) fn db<'r>(
    state: &'r State<AppState>,
) -> std::sync::MutexGuard<'r, rusqlite::Connection> {
    state.db.lock().unwrap_or_else(|e| {
        tracing::warn!("db mutex poisoned in command, recovering: {e}");
        e.into_inner()
    })
}
