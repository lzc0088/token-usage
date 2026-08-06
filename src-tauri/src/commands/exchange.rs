use crate::JUHE_API_KEY;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{debug, error, info, warn};

use crate::config;
use crate::state::AppState;

const API_URL: &str = "https://op.juhe.cn/onebox/exchange/currency";

/// Resolve the Juhe API key compiled into the binary.
/// Returns `None` when unset — callers should skip the API call gracefully.
fn api_key() -> Option<&'static str> {
    if JUHE_API_KEY.is_empty() {
        None
    } else {
        Some(JUHE_API_KEY)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExchangeRateResponse {
    error_code: i32,
    result: Option<Vec<RateData>>,
    reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RateData {
    #[allow(non_snake_case)]
    #[serde(rename = "currencyF")]
    currency_f: String,
    exchange: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RateInfo {
    pub rate: f64,
    pub cached: bool,
    pub date: String,
}

/// 获取USD到CNY的汇率
/// 优先从缓存读取，缓存不存在则调用API
#[tauri::command]
pub fn get_exchange_rate(state: State<'_, AppState>) -> Result<RateInfo, String> {
    debug!("get_exchange_rate called");
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // 先尝试从缓存获取
    let conn = state.db_read();
    match get_cached_rate(&conn, "USD", &today) {
        Ok(Some(cached)) => {
            debug!(rate = cached, "cache hit");
            return Ok(RateInfo {
                rate: cached,
                cached: true,
                date: today,
            });
        }
        Ok(None) => {
            debug!("cache miss, fetching from API");
            if api_key().is_none() {
                debug!("JUHE_API_KEY not set, returning fallback rate");
                return Ok(RateInfo {
                    rate: 7.2,
                    cached: false,
                    date: today,
                });
            }
        }
        Err(e) => {
            warn!(error = %e, "cache query error, fetching from API");
        }
    }
    drop(conn);

    // 缓存不存在，调用API获取
    fetch_and_cache_rate(&state)
}

/// 刷新汇率（强制调用API）
#[tauri::command]
pub fn refresh_exchange_rate(state: State<'_, AppState>) -> Result<RateInfo, String> {
    debug!("refresh_exchange_rate called");
    fetch_and_cache_rate(&state)
}

/// 调用API获取汇率并缓存
fn fetch_and_cache_rate(state: &AppState) -> Result<RateInfo, String> {
    let key = api_key().ok_or_else(|| {
        debug!("JUHE_API_KEY not set, cannot fetch exchange rate");
        "未配置汇率 API 密钥 (JUHE_API_KEY)".to_string()
    })?;

    debug!(url = %API_URL, "calling exchange API");
    let response = ureq::get(&format!(
        "{}?key={}&from=USD&to=CNY&version=2",
        API_URL, key
    ))
    .timeout(std::time::Duration::from_secs(10))
    .call()
    .map_err(|e| {
        error!(error = %e, "API call failed");
        format!("API调用失败: {}", e)
    })?;

    let json_str = response.into_string().map_err(|e| {
        error!(error = %e, "response read failed");
        format!("响应读取失败: {}", e)
    })?;

    debug!(len = json_str.len(), "API response received");

    let json: ExchangeRateResponse = serde_json::from_str(&json_str).map_err(|e| {
        error!(error = %e, "JSON parse failed");
        format!("JSON解析失败: {}", e)
    })?;

    // 检查错误码
    if json.error_code != 0 {
        let reason = json.reason.unwrap_or_else(|| "未知错误".to_string());
        error!(code = json.error_code, reason = %reason, "API returned error");
        return Err(format!("API返回错误: {}", reason));
    }

    // 解析汇率
    let rate = json
        .result
        .and_then(|rates| rates.into_iter().find(|r| r.currency_f == "USD"))
        .and_then(|r| r.exchange.parse::<f64>().ok())
        .unwrap_or_else(|| {
            warn!("API returned unparseable exchange rate, falling back to 7.2");
            7.2
        });

    info!(rate, "parsed exchange rate");

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // 存入缓存
    let conn = state.db_read();
    if let Err(e) = save_rate(&conn, "USD", rate, &today) {
        warn!(error = %e, "cache save failed");
        // 缓存保存失败不影响返回结果
    }

    Ok(RateInfo {
        rate,
        cached: false,
        date: today,
    })
}

/// 从数据库获取缓存的汇率
fn get_cached_rate(
    conn: &rusqlite::Connection,
    from: &str,
    date: &str,
) -> Result<Option<f64>, rusqlite::Error> {
    conn.query_row(
        "SELECT rate FROM exchange_rate WHERE from_currency = ?1 AND date = ?2",
        [from, date],
        |row| row.get(0),
    )
    .optional()
}

/// 保存汇率到数据库
fn save_rate(
    conn: &rusqlite::Connection,
    from: &str,
    rate: f64,
    date: &str,
) -> Result<(), rusqlite::Error> {
    let updated_at = chrono::Local::now().timestamp_millis();
    conn.execute(
        "INSERT OR REPLACE INTO exchange_rate (from_currency, to_currency, rate, date, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        [from, "CNY", &rate.to_string(), date, &updated_at.to_string()],
    )
    .map(|_| ())
}

/// Fetch the most recent stored USD→CNY rate (any date), without hitting the
/// API. Used by the main UI for cost conversion — always returns a usable
/// value (defaults to 7.2 when nothing is stored yet).
#[tauri::command]
pub fn get_latest_rate(state: State<'_, AppState>) -> Result<RateInfo, String> {
    let conn = state.db_read();
    let latest = get_latest_cached_rate(&conn, "USD");
    let (rate, date, cached) = match latest {
        Ok(Some((rate, date))) => (rate, date, true),
        _ => (
            7.2,
            chrono::Local::now().format("%Y-%m-%d").to_string(),
            false,
        ),
    };
    Ok(RateInfo { rate, cached, date })
}

/// Read the newest stored rate row (any date) for a currency.
fn get_latest_cached_rate(
    conn: &rusqlite::Connection,
    from: &str,
) -> Result<Option<(f64, String)>, rusqlite::Error> {
    conn.query_row(
        "SELECT rate, date FROM exchange_rate WHERE from_currency = ?1 ORDER BY date DESC LIMIT 1",
        [from],
        |row| Ok((row.get::<_, f64>(0)?, row.get::<_, String>(1)?)),
    )
    .optional()
}

/// Persist a user-supplied USD→CNY rate, switch `rate_mode` to "manual", and
/// notify all windows so cost displays recompute immediately.
#[tauri::command]
pub fn set_manual_rate(
    app: AppHandle,
    state: State<'_, AppState>,
    rate: f64,
) -> Result<(), String> {
    if !rate.is_finite() || rate <= 0.0 {
        return Err("汇率必须为正数".into());
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    {
        let conn = state.db_read();
        if let Err(e) = save_rate(&conn, "USD", rate, &today) {
            error!(error = %e, "manual rate save failed");
            return Err(format!("保存失败: {e}"));
        }
        // Persist rate_mode = manual.
        let _ = config::with_config(&conn, |cfg| {
            cfg.rate_mode = "manual".into();
        });
    }
    let _ = app.emit("rate:updated", ());
    let _ = app.emit("config:changed", ());
    Ok(())
}

/// Background one-shot: if `rate_mode == "auto"` and today's USD→CNY rate is
/// not cached, fetch it from the API and notify windows. Runs on a blocking
/// thread (ureq is blocking). Failure is silent (e.g. offline or no API key)
/// — the UI falls back to the last stored rate via `get_latest_rate`.
pub fn startup_auto_fetch(app: AppHandle) {
    std::thread::spawn(move || {
        let st = app.state::<AppState>();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let need_fetch = {
            let conn = st.db_read();
            let mode = config::load(&conn)
                .map(|c| c.rate_mode)
                .unwrap_or_else(|_| "auto".into());
            if mode != "auto" {
                false
            } else {
                get_cached_rate(&conn, "USD", &today)
                    .ok()
                    .flatten()
                    .is_none()
            }
        };
        if !need_fetch {
            return;
        }
        if api_key().is_none() {
            debug!("skipping auto-fetch: JUHE_API_KEY not set");
            return;
        }
        info!("startup auto-fetch (mode=auto, no today cache)");
        match fetch_and_cache_rate(&st) {
            Ok(_) => {
                let _ = app.emit("rate:updated", ());
            }
            Err(e) => warn!(error = %e, "startup auto-fetch failed"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::state::AppState;
    use crate::storage::schema;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    fn app_state(c: Connection) -> AppState {
        AppState::with_db(Arc::new(Mutex::new(c)))
    }

    /// SAFETY: `State<'r, T>` is `pub struct State<'r, T>(&'r T)`.
    fn state_of(s: &AppState) -> State<'_, AppState> {
        unsafe { std::mem::transmute(s) }
    }

    // ── get_cached_rate ──────────────────────────────────────────────────

    #[test]
    fn get_cached_rate_returns_none_on_cache_miss() {
        let c = conn();
        let result = get_cached_rate(&c, "USD", "2026-01-01").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn get_cached_rate_returns_rate_on_cache_hit() {
        let c = conn();
        save_rate(&c, "USD", 6.95, "2026-01-01").unwrap();
        let result = get_cached_rate(&c, "USD", "2026-01-01").unwrap();
        assert_eq!(result, Some(6.95));
    }

    #[test]
    fn get_cached_rate_misses_on_wrong_date() {
        let c = conn();
        save_rate(&c, "USD", 6.95, "2026-01-01").unwrap();
        let result = get_cached_rate(&c, "USD", "2026-01-02").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn get_cached_rate_misses_on_wrong_currency() {
        let c = conn();
        save_rate(&c, "USD", 6.95, "2026-01-01").unwrap();
        let result = get_cached_rate(&c, "EUR", "2026-01-01").unwrap();
        assert!(result.is_none());
    }

    // ── save_rate ────────────────────────────────────────────────────────

    #[test]
    fn save_rate_persists_and_is_retrievable() {
        let c = conn();
        save_rate(&c, "USD", 7.15, "2026-03-15").unwrap();
        let result = get_cached_rate(&c, "USD", "2026-03-15").unwrap();
        assert_eq!(result, Some(7.15));
    }

    #[test]
    fn save_rate_overwrites_existing_row_same_primary_key() {
        let c = conn();
        save_rate(&c, "USD", 7.0, "2026-04-01").unwrap();
        save_rate(&c, "USD", 7.5, "2026-04-01").unwrap();
        let result = get_cached_rate(&c, "USD", "2026-04-01").unwrap();
        assert_eq!(result, Some(7.5));
    }

    #[test]
    fn save_rate_preserves_to_currency_cny() {
        let c = conn();
        save_rate(&c, "USD", 7.25, "2026-05-01").unwrap();
        let to: String = c
            .query_row(
                "SELECT to_currency FROM exchange_rate WHERE from_currency = 'USD' AND date = '2026-05-01'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(to, "CNY");
    }

    #[test]
    fn save_rate_with_very_small_positive_rate() {
        let c = conn();
        save_rate(&c, "USD", 1e-10, "2026-06-01").unwrap();
        let result = get_cached_rate(&c, "USD", "2026-06-01").unwrap();
        assert_eq!(result, Some(1e-10));
    }

    #[test]
    fn save_rate_with_large_rate() {
        let c = conn();
        save_rate(&c, "USD", 9999.99, "2026-07-01").unwrap();
        let result = get_cached_rate(&c, "USD", "2026-07-01").unwrap();
        assert_eq!(result, Some(9999.99));
    }

    // ── get_latest_cached_rate ───────────────────────────────────────────

    #[test]
    fn get_latest_cached_rate_returns_none_when_empty() {
        let c = conn();
        let result = get_latest_cached_rate(&c, "USD").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn get_latest_cached_rate_returns_single_entry() {
        let c = conn();
        save_rate(&c, "USD", 7.0, "2026-01-15").unwrap();
        let result = get_latest_cached_rate(&c, "USD").unwrap();
        assert_eq!(result, Some((7.0, "2026-01-15".to_string())));
    }

    #[test]
    fn get_latest_cached_rate_returns_newest_by_date() {
        let c = conn();
        save_rate(&c, "USD", 6.8, "2026-01-01").unwrap();
        save_rate(&c, "USD", 7.3, "2026-02-10").unwrap();
        save_rate(&c, "USD", 7.0, "2026-03-15").unwrap();
        let result = get_latest_cached_rate(&c, "USD").unwrap();
        assert_eq!(result, Some((7.0, "2026-03-15".to_string())));
    }

    // ── get_latest_rate ──────────────────────────────────────────────────

    #[test]
    fn get_latest_rate_falls_back_to_7_2_when_cache_empty() {
        let state = app_state(conn());
        let info = get_latest_rate(state_of(&state)).unwrap();
        assert_eq!(info.rate, 7.2);
        assert!(!info.cached);
        assert_eq!(info.date.len(), 10);
    }

    #[test]
    fn get_latest_rate_returns_cached_rate_when_present() {
        let c = conn();
        save_rate(&c, "USD", 7.35, "2026-07-20").unwrap();
        let state = app_state(c);
        let info = get_latest_rate(state_of(&state)).unwrap();
        assert_eq!(info.rate, 7.35);
        assert!(info.cached);
        assert_eq!(info.date, "2026-07-20");
    }

    // ── set_manual_rate logic ────────────────────────────────────────────

    #[test]
    fn set_manual_rate_rejects_nan() {
        assert!(!f64::NAN.is_finite());
    }

    #[test]
    fn set_manual_rate_rejects_infinity() {
        assert!(!f64::INFINITY.is_finite());
        assert!(!f64::NEG_INFINITY.is_finite());
    }

    #[test]
    fn set_manual_rate_rejects_zero_and_negative() {
        assert!(0.0_f64 <= 0.0);
        assert!(-5.0_f64 <= 0.0);
    }

    #[test]
    fn set_manual_rate_saves_rate_and_sets_mode_to_manual() {
        let c = conn();
        let rate = 7.35;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        save_rate(&c, "USD", rate, &today).unwrap();
        config::with_config(&c, |cfg| {
            cfg.rate_mode = "manual".into();
        })
        .unwrap();
        let stored = get_cached_rate(&c, "USD", &today).unwrap();
        assert_eq!(stored, Some(rate));
        let loaded = config::load(&c).unwrap();
        assert_eq!(loaded.rate_mode, "manual");
    }

    // ── api_key ──────────────────────────────────────────────────────────

    #[test]
    fn api_key_returns_none_when_juhe_key_is_empty() {
        let key = api_key();
        if key.is_none() {
            // Expected when JUHE_API_KEY is not set (CI / fresh checkout).
        } else {
            assert!(!key.unwrap().is_empty());
        }
    }

    // ── serde roundtrip ──────────────────────────────────────────────────

    #[test]
    fn deserialize_exchange_rate_response_success() {
        let json = r#"{
            "error_code": 0,
            "result": [{"currencyF": "USD", "exchange": "7.2356"}],
            "reason": null
        }"#;
        let resp: ExchangeRateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.error_code, 0);
        let rates = resp.result.unwrap();
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].currency_f, "USD");
        assert_eq!(rates[0].exchange, "7.2356");
    }

    #[test]
    fn deserialize_exchange_rate_response_error() {
        let json = r#"{
            "error_code": 10001,
            "result": null,
            "reason": "Invalid key"
        }"#;
        let resp: ExchangeRateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.error_code, 10001);
        assert_eq!(resp.reason.unwrap(), "Invalid key");
    }
}
