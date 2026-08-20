//! OpenCode Cloud quota adapter — local DB + web dual path.
//!
//! Mirrors token-monitor's three-source approach:
//! 1. **Go Local** — read `opencode.db` SQLite, sum `cost` over time windows,
//!    hardcoded limits ($12/5h, $30/week, $60/month). Always available, no network.
//! 2. **Go Web** — fetch `/workspace/<id>/go` HTML page, parse embedded usage.
//!    May fail for SPA pages.
//! 3. **Zen** — call `_server` subscription endpoint, parse rolling/weekly + balance.
//!    May return null for free accounts.
//!
//! Merge priority (limitCollector.js):
//!   Go Web ok → use; else Go Local ok → use; Zen ok → append windows + balance.

use regex::Regex;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::{epoch_to_iso, Quota, QuotaBalance, QuotaStatus, QuotaWindow};
use super::VendorError;

const BASE_URL: &str = "https://opencode.ai";
const SERVER_URL: &str = "https://opencode.ai/_server";
const WORKSPACES_SERVER_ID: &str =
    "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f";
const SUBSCRIPTION_SERVER_ID: &str =
    "7abeebee372f304e050aaaf92be863f4a86490e382f8c79db68fd94040d691b4";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

// Hardcoded Go plan limits (USD) — server-side fixed, not stored locally.
const GO_LIMIT_SESSION: f64 = 12.0; // $12 / 5h
const GO_LIMIT_WEEKLY: f64 = 30.0; // $30 / week
const GO_LIMIT_MONTHLY: f64 = 60.0; // $60 / month

const SESSION_MS: i64 = 5 * 60 * 60 * 1000; // 5h
const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000; // 7d

const PCT_KEYS: &[&str] = &[
    "usagePercent",
    "usedPercent",
    "percentUsed",
    "percent",
    "usage",
];
const RESET_SEC_KEYS: &[&str] = &[
    "resetInSec",
    "resetInSeconds",
    "resetSeconds",
    "resetsInSec",
];

// ---------------------------------------------------------------------------
// HTTP trait
// ---------------------------------------------------------------------------

pub struct Response {
    pub status: u16,
    pub text: String,
}

pub trait Http {
    fn call(
        &self,
        method: &str,
        url: &str,
        cookie: &str,
        extra: &[(&str, &str)],
        body: Option<&str>,
    ) -> Result<Response, VendorError>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn sanitize_cookie(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let re = Regex::new(r"(?i)^cookie\s*:\s*").unwrap();
    let stripped = re.replace(trimmed, "").trim().to_string();
    let cleaned = stripped
        .split(';')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    if cleaned.is_empty() {
        return String::new();
    }
    if !cleaned.contains('=') {
        format!("auth={cleaned}")
    } else {
        cleaned
    }
}

fn server_instance() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("server-fn:{:032x}", nanos)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn looks_signed_out(text: &str) -> bool {
    let l = text.to_lowercase();
    l.contains("login")
        || l.contains("sign in")
        || l.contains("auth/authorize")
        || l.contains("not associated with an account")
        || l.contains("actor of type \"public\"")
}

// ---------------------------------------------------------------------------
// §1  Go Local — read opencode.db
// ---------------------------------------------------------------------------

/// Resolve the opencode data directory.
fn resolve_data_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("opencode");
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".local").join("share").join("opencode")
}

/// `opencode.db` or `opencode-<channel>.db` (excludes WAL/SHM/journal).
fn is_opencode_db(name: &str) -> bool {
    if !name.ends_with(".db") {
        return false;
    }
    let stem = &name[..name.len() - 3];
    if stem == "opencode" {
        return true;
    }
    match stem.strip_prefix("opencode-") {
        Some(ch) => {
            !ch.is_empty()
                && ch
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        }
        None => false,
    }
}

/// Discover all opencode.db paths, sorted.
fn discover_db_paths() -> Vec<PathBuf> {
    if let Ok(override_path) = std::env::var("OPENCODE_DB") {
        let p = PathBuf::from(&override_path);
        if p.is_file() {
            return vec![p];
        }
    }
    let dir = resolve_data_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().map(is_opencode_db).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths
}

/// One cost row from the message table.
struct CostRow {
    created_ms: i64,
    cost: f64,
}

/// Query opencode-go rows from a single DB file.
fn read_go_rows(db_path: &std::path::Path) -> Result<Vec<CostRow>, rusqlite::Error> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch("PRAGMA busy_timeout = 250")?;
    let mut stmt = conn.prepare(
        "SELECT CAST(COALESCE(json_extract(data,'$.time.created'), time_created) AS INTEGER),
                CAST(json_extract(data,'$.cost') AS REAL)
         FROM message
         WHERE json_valid(data)
           AND json_extract(data,'$.providerID') = 'opencode-go'
           AND json_extract(data,'$.role') = 'assistant'
           AND json_type(data,'$.cost') IN ('integer','real')",
    )?;
    let rows: Vec<CostRow> = stmt
        .query_map([], |row| {
            Ok(CostRow {
                created_ms: row.get(0)?,
                cost: row.get(1)?,
            })
        })?
        .filter_map(|r| r.ok())
        .filter(|r| r.created_ms > 0 && r.cost.is_finite() && r.cost >= 0.0)
        .collect();
    Ok(rows)
}

/// Compute the UTC Monday 00:00 timestamp for the week containing `now_ms`.
fn week_start_ms(now: i64) -> i64 {
    let days_since_epoch = now / 86_400_000;
    // 1970-01-01 was a Thursday (4, 0-indexed Mon=0).
    let day_of_week = (days_since_epoch + 4) % 7; // Mon=0..Sun=6
    let monday = now - day_of_week * 86_400_000;
    monday - (monday % 86_400_000)
}

/// Calendar month bounds `[start_ms, end_ms)` anchored to the earliest usage.
fn month_bounds_ms(now: i64, anchor_ms: Option<i64>) -> (i64, i64) {
    use chrono::{Datelike, TimeZone, Utc};
    let now_dt = Utc.timestamp_millis_opt(now).single().unwrap_or_default();
    let (y, m) = match anchor_ms {
        Some(a) if a > 0 => {
            let a_dt = Utc.timestamp_millis_opt(a).single().unwrap_or_default();
            (a_dt.year(), a_dt.month())
        }
        _ => (now_dt.year(), now_dt.month()),
    };
    let start = Utc
        .with_ymd_and_hms(y, m, 1, 0, 0, 0)
        .unwrap()
        .timestamp_millis();
    let (ey, em) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let end = Utc
        .with_ymd_and_hms(ey, em, 1, 0, 0, 0)
        .unwrap()
        .timestamp_millis();
    (start, end)
}

fn sum_cost(rows: &[CostRow], start_ms: i64, end_ms: i64) -> f64 {
    rows.iter()
        .filter(|r| r.created_ms >= start_ms && r.created_ms < end_ms)
        .map(|r| r.cost)
        .sum()
}

/// Collect Go usage from local DB. Returns windows if any opencode-go rows exist.
fn collect_go_local() -> Vec<QuotaWindow> {
    let paths = discover_db_paths();
    if paths.is_empty() {
        return vec![];
    }
    let now = now_ms();
    let mut rows: Vec<CostRow> = Vec::new();
    for p in &paths {
        match read_go_rows(p) {
            Ok(r) => rows.extend(r),
            Err(_) => continue,
        }
    }
    if rows.is_empty() {
        return vec![];
    }
    let earliest = rows.iter().map(|r| r.created_ms).min().unwrap_or(now);
    let session_start = now - SESSION_MS;
    let week_start = week_start_ms(now);
    let (month_start, month_end) = month_bounds_ms(now, Some(earliest));

    let session_used = sum_cost(&rows, session_start, now);
    let weekly_used = sum_cost(&rows, week_start, week_start + WEEK_MS);
    let monthly_used = sum_cost(&rows, month_start, month_end);

    // Session reset: oldest row in window + 5h; if none, now + 5h.
    let session_oldest = rows
        .iter()
        .filter(|r| r.created_ms >= session_start && r.created_ms < now)
        .map(|r| r.created_ms)
        .min()
        .unwrap_or(now);
    let session_reset = session_oldest + SESSION_MS;
    let weekly_reset = week_start + WEEK_MS;

    let pct = |used: f64, limit: f64| -> f64 {
        if limit > 0.0 {
            (used / limit * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        }
    };

    let s = QuotaWindow {
        label: "5h".into(),
        used_pct: pct(session_used, GO_LIMIT_SESSION),
        resets_at: epoch_to_iso(session_reset as f64),
        used_value: Some((session_used * 100.0).round() / 100.0),
        total_value: Some(GO_LIMIT_SESSION),
        ..Default::default()
    };
    let w = QuotaWindow {
        label: "周".into(),
        used_pct: pct(weekly_used, GO_LIMIT_WEEKLY),
        resets_at: epoch_to_iso(weekly_reset as f64),
        used_value: Some((weekly_used * 100.0).round() / 100.0),
        total_value: Some(GO_LIMIT_WEEKLY),
        ..Default::default()
    };
    let m = QuotaWindow {
        label: "月".into(),
        used_pct: pct(monthly_used, GO_LIMIT_MONTHLY),
        resets_at: epoch_to_iso(month_end as f64),
        used_value: Some((monthly_used * 100.0).round() / 100.0),
        total_value: Some(GO_LIMIT_MONTHLY),
        ..Default::default()
    };
    vec![s, w, m]
}

// ---------------------------------------------------------------------------
// §2  Web — workspace resolution + subscription fetch
// ---------------------------------------------------------------------------

fn parse_workspace_id(text: &str) -> Option<String> {
    let re = Regex::new(r"wrk_[A-Za-z0-9]+").unwrap();
    re.find(text).map(|m| m.as_str().to_string())
}

fn workspaces_server_url() -> String {
    format!("{}?id={}", SERVER_URL, WORKSPACES_SERVER_ID)
}

fn resolve_workspace(http: &dyn Http, cookie: &str) -> Result<String, VendorError> {
    let inst = server_instance();
    let headers: &[(&str, &str)] = &[
        ("X-Server-Id", WORKSPACES_SERVER_ID),
        ("X-Server-Instance", inst.as_str()),
        ("Origin", BASE_URL),
        ("Referer", BASE_URL),
        (
            "Accept",
            "text/javascript, application/json;q=0.9, */*;q=0.8",
        ),
        ("User-Agent", USER_AGENT),
    ];
    let url = workspaces_server_url();
    let resp = http.call("GET", &url, cookie, headers, None)?;
    if resp.status == 401 || resp.status == 403 || looks_signed_out(&resp.text) {
        return Err(VendorError::Auth("OpenCode 会话已失效".into()));
    }
    if let Some(id) = parse_workspace_id(&resp.text) {
        return Ok(id);
    }
    let resp = http.call("POST", &url, cookie, headers, Some("[]"))?;
    if looks_signed_out(&resp.text) {
        return Err(VendorError::Auth("OpenCode 会话已失效".into()));
    }
    match parse_workspace_id(&resp.text) {
        Some(id) => Ok(id),
        None => Err(VendorError::Empty),
    }
}

/// Fetch subscription via `_server` TanStack endpoint (token-monitor `fetchZen`).
fn fetch_subscription(
    http: &dyn Http,
    cookie: &str,
    workspace_id: &str,
) -> Result<String, VendorError> {
    let args_json = serde_json::to_string(&[workspace_id]).unwrap_or_default();
    let url = format!(
        "{}?id={}&args={}",
        SERVER_URL,
        SUBSCRIPTION_SERVER_ID,
        urlencode(&args_json)
    );
    let referer = format!("{}/workspace/{}/billing", BASE_URL, workspace_id);
    let inst = server_instance();
    let headers: &[(&str, &str)] = &[
        ("X-Server-Id", SUBSCRIPTION_SERVER_ID),
        ("X-Server-Instance", inst.as_str()),
        ("Origin", BASE_URL),
        ("Referer", referer.as_str()),
        (
            "Accept",
            "text/javascript, application/json;q=0.9, */*;q=0.8",
        ),
        ("User-Agent", USER_AGENT),
    ];
    let resp = http.call("GET", &url, cookie, headers, None)?;
    if resp.status == 429 {
        return Err(VendorError::Network("status code 429".into()));
    }
    if resp.status == 401 || resp.status == 403 || looks_signed_out(&resp.text) {
        return Err(VendorError::Auth("status code 401".into()));
    }
    if resp.status != 200 {
        return Err(VendorError::Network(format!("status code {}", resp.status)));
    }
    let trimmed = resp.text.trim();
    let is_explicit_null = trimmed.eq_ignore_ascii_case("null")
        || trimmed.ends_with(",null)")
        || trimmed.ends_with("=null)");
    if parse_go_usage(&resp.text).is_empty() && !looks_signed_out(&resp.text) && !is_explicit_null {
        let resp = http.call("POST", SERVER_URL, cookie, headers, Some(&args_json))?;
        if resp.status == 401 || resp.status == 403 || looks_signed_out(&resp.text) {
            return Err(VendorError::Auth("status code 401".into()));
        }
        return Ok(resp.text);
    }
    Ok(resp.text)
}

// ---------------------------------------------------------------------------
// §3  Parsing (shared by web + fallback)
// ---------------------------------------------------------------------------

struct RawWindow {
    label: &'static str,
    used_pct: f64,
    reset_secs: u64,
}

fn as_num(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
}

fn pick_num(obj: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    let map = obj.as_object()?;
    for k in keys {
        if let Some(v) = map.get(*k) {
            if let Some(n) = as_num(v) {
                return Some(n);
            }
        }
    }
    None
}

fn find_by_keyword<'a>(obj: &'a serde_json::Value, keyword: &str) -> Option<&'a serde_json::Value> {
    fn walk<'a>(obj: &'a serde_json::Value, kw: &str, depth: u32) -> Option<&'a serde_json::Value> {
        if depth > 4 {
            return None;
        }
        if let Some(map) = obj.as_object() {
            for (k, v) in map {
                if v.is_object() && k.to_lowercase().contains(kw) {
                    return Some(v);
                }
            }
            for v in map.values() {
                if v.is_object() {
                    if let Some(f) = walk(v, kw, depth + 1) {
                        return Some(f);
                    }
                }
            }
        }
        None
    }
    walk(obj, keyword, 0)
}

fn parse_window_obj(obj: &serde_json::Value) -> Option<RawWindow> {
    let mut pct = pick_num(obj, PCT_KEYS);
    if pct.is_none() {
        let used = pick_num(obj, &["used", "consumed"]);
        let limit = pick_num(obj, &["limit", "total", "quota", "max", "cap"]);
        if let (Some(u), Some(l)) = (used, limit) {
            if l > 0.0 {
                pct = Some(u / l * 100.0);
            }
        }
    }
    let mut p = pct?;
    if (0.0..=1.0).contains(&p) {
        p *= 100.0;
    }
    let reset_secs = pick_num(obj, RESET_SEC_KEYS)
        .map(|s| if s > 0.0 { s as u64 } else { 0 })
        .unwrap_or(0);
    Some(RawWindow {
        label: "",
        used_pct: p.clamp(0.0, 100.0),
        reset_secs,
    })
}

fn extract_window_regex(text: &str, window_key: &str, label: &'static str) -> Option<RawWindow> {
    let pct_pat = format!(
        r"{}[^}}]*?usagePercent\s*[:=]\s*([0-9]+(?:\.[0-9]+)?)",
        window_key
    );
    let pm = Regex::new(&pct_pat).ok()?.captures(text)?;
    let pct: f64 = pm.get(1)?.as_str().parse().ok()?;
    let reset_pat = format!(r"{}[^}}]*?resetInSec\s*[:=]\s*([0-9]+)", window_key);
    let reset_secs = Regex::new(&reset_pat)
        .ok()
        .and_then(|r| r.captures(text))
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u64>().ok())
        .unwrap_or(0);
    let p = if (0.0..=1.0).contains(&pct) {
        pct * 100.0
    } else {
        pct
    };
    Some(RawWindow {
        label,
        used_pct: p.clamp(0.0, 100.0),
        reset_secs,
    })
}

/// Parse rolling/weekly(/monthly) from JSON or regex fallback.
fn parse_go_usage(text: &str) -> Vec<RawWindow> {
    let from_json = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .filter(|v| v.is_object())
        .map(|root| {
            let rolling = find_by_keyword(&root, "rolling").and_then(parse_window_obj);
            let weekly = find_by_keyword(&root, "weekly")
                .or_else(|| find_by_keyword(&root, "week"))
                .and_then(parse_window_obj);
            let mut out = Vec::new();
            if let (Some(mut r), Some(mut w)) = (rolling, weekly) {
                r.label = "5h";
                w.label = "周";
                out.push(r);
                out.push(w);
                if let Some(mut m) = find_by_keyword(&root, "monthly")
                    .or_else(|| find_by_keyword(&root, "month"))
                    .and_then(parse_window_obj)
                {
                    m.label = "月";
                    out.push(m);
                }
            }
            out
        })
        .unwrap_or_default();
    if !from_json.is_empty() {
        return from_json;
    }
    let mut out = Vec::new();
    if let Some(w) = extract_window_regex(text, "rollingUsage", "5h") {
        out.push(w);
    }
    if let Some(w) = extract_window_regex(text, "weeklyUsage", "周") {
        out.push(w);
    }
    if let Some(w) = extract_window_regex(text, "monthlyUsage", "月") {
        out.push(w);
    }
    out
}

fn extract_balance_usd(text: &str) -> Option<f64> {
    let re = Regex::new(r"(?i)(?:balanceUSD|currentBalance|zenBalance|balanceUsd)[^0-9\-]{0,20}([0-9]+(?:\.[0-9]+)?)").ok()?;
    re.captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<f64>().ok())
}

/// Convert web RawWindows (with resetInSec) → QuotaWindows.
fn raw_windows_to_quota(raws: Vec<RawWindow>) -> Vec<QuotaWindow> {
    let now = now_ms();
    raws.into_iter()
        .map(|r| {
            let resets_ms = now + (r.reset_secs as i64) * 1000;
            QuotaWindow {
                label: r.label.into(),
                used_pct: r.used_pct,
                resets_at: epoch_to_iso(resets_ms as f64),
                ..Default::default()
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// §4  Main fetch — merge local + web
// ---------------------------------------------------------------------------

pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let raw = serde_json::from_str::<serde_json::Value>(credential)
        .ok()
        .and_then(|v| {
            v.get("cookie")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| credential.to_string());
    let cookie = sanitize_cookie(&raw);
    if cookie.is_empty() {
        return Err(VendorError::Parse("缺少 auth 登录令牌".into()));
    }

    // ① Go Local — always available, no network.
    let go_local = collect_go_local();

    // ② Go Web + Zen — sequential (both need same HTTP).
    let mut zen_windows: Vec<QuotaWindow> = Vec::new();
    let mut balance_usd: Option<f64> = None;

    let workspace_id = match resolve_workspace(http, &cookie) {
        Ok(id) => id,
        Err(_) => {
            // No workspace → skip web, fall through to local-only.
            if go_local.is_empty() {
                return Err(VendorError::Empty);
            }
            return build_quota(go_local, None, None);
        }
    };

    // ②a Go Web — fetch Go page HTML (may be SPA).
    let go_web = match fetch_go_page_html(http, &cookie, &workspace_id) {
        Ok(html) => {
            let raws = parse_go_usage(&html);
            raw_windows_to_quota(raws)
        }
        Err(_) => vec![],
    };

    // ②b Zen — _server subscription.
    if let Ok(text) = fetch_subscription(http, &cookie, &workspace_id) {
        let raws = parse_go_usage(&text);
        balance_usd = extract_balance_usd(&text);
        zen_windows = raw_windows_to_quota(raws);
    }

    // ③ Merge: Go Web → Go Local → (fallback); Zen appends.
    let mut windows = if !go_web.is_empty() {
        go_web
    } else if !go_local.is_empty() {
        go_local
    } else {
        vec![]
    };
    windows.extend(zen_windows);

    let balance = balance_usd.map(|amt| QuotaBalance {
        amount: (amt * 100.0).round() / 100.0,
        currency: "USD".into(),
        today_consumption: None,
        month_consumption: None,
    });

    let has_data = !windows.is_empty() || balance.is_some();
    let error = if has_data {
        None
    } else {
        Some("暂无可用的额度数据".into())
    };
    build_quota(windows, balance, error)
}

/// Fetch the Go page HTML as a fallback for embedded SSR data.
fn fetch_go_page_html(
    http: &dyn Http,
    cookie: &str,
    workspace_id: &str,
) -> Result<String, VendorError> {
    let url = format!("{}/workspace/{}/go", BASE_URL, workspace_id);
    let headers: &[(&str, &str)] = &[
        (
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
        ("Referer", BASE_URL),
        ("User-Agent", USER_AGENT),
    ];
    let resp = http.call("GET", &url, cookie, headers, None)?;
    if resp.status == 429 {
        return Err(VendorError::Network("status code 429".into()));
    }
    if resp.status == 401 || resp.status == 403 || looks_signed_out(&resp.text) {
        return Err(VendorError::Auth("status code 401".into()));
    }
    if resp.status != 200 {
        return Err(VendorError::Network(format!("status code {}", resp.status)));
    }
    Ok(resp.text)
}

fn build_quota(
    windows: Vec<QuotaWindow>,
    balance: Option<QuotaBalance>,
    error: Option<String>,
) -> Result<Quota, VendorError> {
    let used_pct = windows.iter().map(|w| w.used_pct).fold(0.0f64, f64::max);
    Ok(Quota {
        site: None,
        vendor: "opencode".into(),
        plan_label: Some("Go".into()),
        status: QuotaStatus::from_used_pct(used_pct),
        windows,
        balance,
        refreshed_at: None,
        error,
        cookie_error: None,
        expires_at: None,
    })
}

pub async fn fetch(credential: &str) -> Result<Quota, VendorError> {
    let cred = credential.to_string();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &cred))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

struct UreqHttp;
impl Http for UreqHttp {
    fn call(
        &self,
        method: &str,
        url: &str,
        cookie: &str,
        extra: &[(&str, &str)],
        body: Option<&str>,
    ) -> Result<Response, VendorError> {
        let m = method.to_ascii_uppercase();
        let req = if m == "POST" {
            let mut r = ureq::post(url);
            for (k, v) in extra {
                r = r.set(k, v);
            }
            r.set("Cookie", cookie)
                .set("User-Agent", USER_AGENT)
                .set("Content-Type", "application/json")
        } else {
            let mut r = ureq::get(url);
            for (k, v) in extra {
                r = r.set(k, v);
            }
            r.set("Cookie", cookie).set("User-Agent", USER_AGENT)
        };
        let resp = if m == "POST" {
            req.send_string(body.unwrap_or("[]"))
        } else {
            req.call()
        };
        match resp {
            Ok(r) => {
                let status = r.status();
                let text = r.into_string().unwrap_or_default();
                Ok(Response { status, text })
            }
            Err(ureq::Error::Status(code, r)) => {
                let text = r.into_string().unwrap_or_default();
                Ok(Response { status: code, text })
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_wraps_bare_value_as_auth() {
        assert_eq!(sanitize_cookie("abc123"), "auth=abc123");
        assert_eq!(sanitize_cookie("auth=abc123"), "auth=abc123");
        assert_eq!(sanitize_cookie("Cookie: auth=xyz"), "auth=xyz");
        assert_eq!(sanitize_cookie("a=1; b=2"), "a=1; b=2");
        assert_eq!(sanitize_cookie(""), "");
    }

    #[test]
    fn parse_workspace_id_finds_wrk_token() {
        assert_eq!(
            parse_workspace_id(r#"other stuff id="wrk_abc123" tail"#).as_deref(),
            Some("wrk_abc123")
        );
        assert!(parse_workspace_id("no workspace here").is_none());
    }

    #[test]
    fn extract_window_regex_parses_usage_and_reset() {
        let text = r#"rollingUsage:{usagePercent:42.5,resetInSec:1800} weeklyUsage:{usagePercent:10,resetInSec:3600}"#;
        let w = extract_window_regex(text, "rollingUsage", "5h").unwrap();
        assert!((w.used_pct - 42.5).abs() < 1e-6);
        assert_eq!(w.reset_secs, 1800);
        let w2 = extract_window_regex(text, "weeklyUsage", "周").unwrap();
        assert!((w2.used_pct - 10.0).abs() < 1e-6);
    }

    #[test]
    fn parse_go_usage_from_regex_text() {
        let text = r#"foo rollingUsage:{usagePercent:25,resetInSec:600} bar weeklyUsage:{usagePercent:50,resetInSec:7200} monthlyUsage:{usagePercent:30,resetInSec:999}"#;
        let w = parse_go_usage(text);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].label, "5h");
        assert!((w[0].used_pct - 25.0).abs() < 1e-6);
        assert_eq!(w[1].label, "周");
        assert_eq!(w[2].label, "月");
    }

    #[test]
    fn parse_go_usage_from_json() {
        let text = r#"{"rollingUsage":{"usagePercent":15,"resetInSec":300},"weeklyUsage":{"usagePercent":40,"resetInSec":3600}}"#;
        let w = parse_go_usage(text);
        assert_eq!(w.len(), 2);
        assert!((w[0].used_pct - 15.0).abs() < 1e-6);
        assert_eq!(w[0].label, "5h");
        assert_eq!(w[1].label, "周");
    }

    #[test]
    fn extract_balance_usd_from_text() {
        assert!((extract_balance_usd(r#"balanceUSD:42.5"#).unwrap() - 42.5).abs() < 1e-6);
        assert!((extract_balance_usd(r#"currentBalance: 100"#).unwrap() - 100.0).abs() < 1e-6);
        assert!(extract_balance_usd("no balance here").is_none());
    }

    #[test]
    fn is_opencode_db_matches_patterns() {
        assert!(is_opencode_db("opencode.db"));
        assert!(is_opencode_db("opencode-go.db"));
        assert!(is_opencode_db("opencode-staging.db"));
        assert!(!is_opencode_db("opencode.db-wal"));
        assert!(!is_opencode_db("opencode.db-shm"));
        assert!(!is_opencode_db("opencode-.db"));
        assert!(!is_opencode_db("other.db"));
    }

    struct Mock {
        workspace_body: &'static str,
        workspace_status: u16,
        sub_body: &'static str,
        sub_status: u16,
    }
    impl Http for Mock {
        fn call(
            &self,
            _: &str,
            url: &str,
            _: &str,
            _: &[(&str, &str)],
            _: Option<&str>,
        ) -> Result<Response, VendorError> {
            if url.contains("/_server") {
                if url.contains("args=") || url.contains("id=7abeebee") {
                    Ok(Response {
                        status: self.sub_status,
                        text: self.sub_body.into(),
                    })
                } else {
                    Ok(Response {
                        status: self.workspace_status,
                        text: self.workspace_body.into(),
                    })
                }
            } else {
                // Go page HTML
                Ok(Response {
                    status: 200,
                    text: String::new(),
                })
            }
        }
    }

    #[test]
    fn fetch_with_returns_session_weekly_monthly_from_web() {
        let mock = Mock {
            workspace_body: r#"stuff id="wrk_test123""#,
            workspace_status: 200,
            sub_body: r#"((self.$R=self.$R||{})["server-fn:xxx"]={rollingUsage:{usagePercent:20,resetInSec:600},weeklyUsage:{usagePercent:40,resetInSec:7200},monthlyUsage:{usagePercent:10,resetInSec:99999}})"#,
            sub_status: 200,
        };
        let q = fetch_with(&mock, "my-auth-token").unwrap();
        assert_eq!(q.vendor, "opencode");
        assert_eq!(q.plan_label.as_deref(), Some("Go"));
        assert_eq!(q.windows.len(), 3);
        assert_eq!(q.windows[0].label, "5h");
        assert!((q.windows[0].used_pct - 20.0).abs() < 1e-6);
    }

    #[test]
    fn fetch_with_null_subscription_returns_error() {
        let mock = Mock {
            workspace_body: r#"id="wrk_test""#,
            workspace_status: 200,
            sub_body: r#"((self.$R=self.$R||{})["server-fn:xxx"]=[],null)"#,
            sub_status: 200,
        };
        // Subscription returns null (free account) and no local DB → error instead of "待实现".
        let q = fetch_with(&mock, "auth=tok").unwrap();
        assert_eq!(q.vendor, "opencode");
        assert!(q.windows.is_empty());
        assert!(q.error.is_some());
        assert_eq!(q.plan_label.as_deref(), Some("Go"));
    }

    #[test]
    fn fetch_with_zen_extracts_balance() {
        let mock = Mock {
            workspace_body: r#"id="wrk_test""#,
            workspace_status: 200,
            sub_body: r#"rollingUsage:{usagePercent:5,resetInSec:100} weeklyUsage:{usagePercent:10,resetInSec:200} balanceUSD:42.5"#,
            sub_status: 200,
        };
        let q = fetch_with(&mock, "auth=tok").unwrap();
        assert!(q.balance.is_some());
        assert!((q.balance.unwrap().amount - 42.5).abs() < 1e-6);
    }

    #[test]
    fn fetch_with_rejects_empty_credential() {
        struct M;
        impl Http for M {
            fn call(
                &self,
                _: &str,
                _: &str,
                _: &str,
                _: &[(&str, &str)],
                _: Option<&str>,
            ) -> Result<Response, VendorError> {
                unreachable!()
            }
        }
        let err = fetch_with(&M, "  ").unwrap_err();
        assert!(matches!(err, VendorError::Parse(_)));
    }

    #[test]
    fn fetch_with_signed_out_returns_empty_error() {
        let mock = Mock {
            workspace_body: "please sign in to continue",
            workspace_status: 200,
            sub_body: "",
            sub_status: 200,
        };
        // Workspace resolution fails (signed out) and no local DB → Empty error.
        assert!(matches!(
            fetch_with(&mock, "auth=expired"),
            Err(VendorError::Empty)
        ));
    }

    #[test]
    fn week_start_ms_returns_monday_utc() {
        // Pick a known Wednesday: 2026-07-29 15:00 UTC = 1785250800000.
        // Its Monday should be 2026-07-27 00:00 UTC.
        let wed_15h = 1785250800000i64;
        let result = week_start_ms(wed_15h);
        // Verify result is a Monday: (days + 4) % 7 == 0  (Mon=0, 1970-01-01=Thu=4).
        let days = result / 86_400_000;
        assert_eq!((days + 4) % 7, 0, "result must be a Monday");
        // Verify result is ≤ the input.
        assert!(result <= wed_15h);
        // Verify result + 7d > input (i.e., input is in this week).
        assert!(result + 7 * 86_400_000 > wed_15h);
    }
}
