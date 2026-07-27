use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::config;
use crate::state::AppState;

const API_URL: &str = "http://op.juhe.cn/onebox/exchange/currency";
const API_KEY: &str = "ff2dfbf15adea35b97052974ede269fd";

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
    eprintln!("[exchange] get_exchange_rate called");
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // 先尝试从缓存获取
    let conn = state.db.lock().expect("db poisoned");
    match get_cached_rate(&conn, "USD", &today) {
        Ok(Some(cached)) => {
            eprintln!("[exchange] cache hit: rate={}", cached);
            return Ok(RateInfo {
                rate: cached,
                cached: true,
                date: today,
            });
        }
        Ok(None) => {
            eprintln!("[exchange] cache miss, fetching from API");
        }
        Err(e) => {
            eprintln!("[exchange] cache query error: {}, fetching from API", e);
        }
    }
    drop(conn);

    // 缓存不存在，调用API获取
    fetch_and_cache_rate(&state)
}

/// 刷新汇率（强制调用API）
#[tauri::command]
pub fn refresh_exchange_rate(state: State<'_, AppState>) -> Result<RateInfo, String> {
    eprintln!("[exchange] refresh_exchange_rate called");
    fetch_and_cache_rate(&state)
}

/// 调用API获取汇率并缓存
fn fetch_and_cache_rate(state: &AppState) -> Result<RateInfo, String> {
    let url = format!("{}?key={}&from=USD&to=CNY&version=2", API_URL, API_KEY);
    eprintln!("[exchange] calling API: {}", url);

    // 调用HTTP API（使用ureq）
    let response = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| {
            eprintln!("[exchange] API call failed: {}", e);
            format!("API调用失败: {}", e)
        })?;

    let json_str = response.into_string().map_err(|e| {
        eprintln!("[exchange] response read failed: {}", e);
        format!("响应读取失败: {}", e)
    })?;

    eprintln!("[exchange] API response length: {}", json_str.len());

    let json: ExchangeRateResponse = serde_json::from_str(&json_str).map_err(|e| {
        eprintln!("[exchange] JSON parse failed: {} | raw: {}", e, json_str);
        format!("JSON解析失败: {}", e)
    })?;

    // 检查错误码
    if json.error_code != 0 {
        let reason = json.reason.unwrap_or_else(|| "未知错误".to_string());
        eprintln!(
            "[exchange] API error: code={}, reason={}",
            json.error_code, reason
        );
        return Err(format!("API返回错误: {}", reason));
    }

    // 解析汇率
    let rate = json
        .result
        .and_then(|rates| rates.into_iter().find(|r| r.currency_f == "USD"))
        .and_then(|r| r.exchange.parse::<f64>().ok())
        .unwrap_or(1.0);

    eprintln!("[exchange] parsed rate: {}", rate);

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // 存入缓存
    let conn = state.db.lock().expect("db poisoned");
    if let Err(e) = save_rate(&conn, "USD", rate, &today) {
        eprintln!("[exchange] cache save failed: {}", e);
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
    let conn = state.db.lock().expect("db poisoned");
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
        let conn = state.db.lock().expect("db poisoned");
        if let Err(e) = save_rate(&conn, "USD", rate, &today) {
            eprintln!("[exchange] manual save failed: {e}");
            return Err(format!("保存失败: {e}"));
        }
        // Persist rate_mode = manual.
        let mut cfg = config::load(&conn).unwrap_or_default();
        cfg.rate_mode = "manual".into();
        if let Err(e) = config::save(&conn, &cfg) {
            eprintln!("[exchange] persist rate_mode failed: {e}");
        }
    }
    let _ = app.emit("rate:updated", ());
    let _ = app.emit("config:changed", ());
    Ok(())
}

/// Background one-shot: if `rate_mode == "auto"` and today's USD→CNY rate is
/// not cached, fetch it from the API and notify windows. Runs on a detached
/// thread (ureq is blocking). Failure is silent (e.g. offline) — the UI falls
/// back to the last stored rate via `get_latest_rate`.
pub fn startup_auto_fetch(app: AppHandle) {
    std::thread::spawn(move || {
        let st = app.state::<AppState>();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let need_fetch = {
            let conn = st.db.lock().expect("db poisoned");
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
        eprintln!("[exchange] startup auto-fetch (mode=auto, no today cache)");
        match fetch_and_cache_rate(&st) {
            Ok(_) => {
                let _ = app.emit("rate:updated", ());
            }
            Err(e) => eprintln!("[exchange] startup auto-fetch failed: {e}"),
        }
    });
}
