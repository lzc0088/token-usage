//! WorkBuddy (Tencent CodeBuddy) quota adapter — local app session detection.
//!
//! Mirrors token-monitor's `workbuddyLocalAuth.js` + `workbuddyLimits.js`:
//! 1. Read the WorkBuddy desktop app's session file
//!    (`~/Library/Application Support/CodeBuddyExtension/Data/Public/auth/
//!    workbuddy-desktop.info` on macOS). A `.logged-out` marker file next to
//!    it means signed out. `expiresAt` (ms) gates staleness.
//! 2. Call the billing API at `copilot.tencent.com`:
//!    - Personal: POST /v2/billing/meter/get-user-resource — aggregates the
//!      active (Status=0) resource packages into one Credits window.
//!    - Enterprise: POST /v2/billing/meter/get-enterprise-user-usage —
//!      limitNum/credit with optional cycleResetTime. limitNum=-1 = unlimited.
//!
//! No credential is required from the user (subscription/auto-detect like
//! Claude Code); the stored credential string, when present, is ignored.

use std::path::PathBuf;

use super::types::{epoch_to_iso, Quota, QuotaStatus, QuotaWindow, QuotaWindowSubItem};
use super::VendorError;

const ENDPOINT: &str = "https://copilot.tencent.com";
const PERSONAL_PATH: &str = "/v2/billing/meter/get-user-resource";
const ENTERPRISE_PATH: &str = "/v2/billing/meter/get-enterprise-user-usage";
const PRODUCT_CODE: &str = "p_tcaca";
/// Session-expiry skew — treat sessions expiring within 30s as expired.
const SESSION_EXPIRY_SKEW_MS: i64 = 30_000;
const AUTH_FILE_MAX_BYTES: u64 = 1024 * 1024;

// ---------------------------------------------------------------------------
// Local session file
// ---------------------------------------------------------------------------

/// The WorkBuddy desktop app's auth directory for this platform.
/// macOS / Windows only (mirrors token-monitor's supported platforms).
fn auth_directories() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        match dirs::home_dir() {
            Some(home) => vec![home
                .join("Library")
                .join("Application Support")
                .join("CodeBuddyExtension")
                .join("Data")
                .join("Public")
                .join("auth")],
            None => Vec::new(),
        }
    }
    #[cfg(target_os = "windows")]
    {
        let home = dirs::home_dir();
        let local = std::env::var("LOCALAPPDATA")
            .ok()
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join("AppData").join("Local")));
        let roaming = std::env::var("APPDATA")
            .ok()
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join("AppData").join("Roaming")));
        let mut dirs: Vec<PathBuf> = Vec::new();
        for base in [local, roaming].into_iter().flatten() {
            let d = base
                .join("CodeBuddyExtension")
                .join("Data")
                .join("Public")
                .join("auth");
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
        return dirs;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Vec::new()
}

/// A parsed WorkBuddy desktop session.
#[derive(Debug, Clone, PartialEq)]
struct LocalSession {
    access_token: String,
    user_id: String,
    enterprise_id: String,
    department_info: String,
    domain: String,
    /// Epoch millis of token expiry (None = no expiry recorded).
    expires_at_ms: Option<i64>,
}

impl LocalSession {
    fn is_enterprise(&self) -> bool {
        !self.enterprise_id.is_empty()
    }
}

/// Read + parse the session file. `None` when: unsupported platform, file
/// missing, logout marker present, JSON malformed, or required fields empty.
fn read_session() -> Option<LocalSession> {
    for dir in auth_directories() {
        let file = dir.join("workbuddy-desktop.info");
        let logout_marker = dir.join("workbuddy-desktop.info.logged-out");
        let has_canonical = file.exists() || logout_marker.exists();
        if !has_canonical {
            continue;
        }
        if logout_marker.exists() {
            return None;
        }
        // Symlink check — never follow (credential-file hygiene).
        let meta = std::fs::metadata(&file).ok()?;
        if !meta.is_file() {
            return None;
        }
        if meta.len() > AUTH_FILE_MAX_BYTES {
            return None;
        }
        let raw = std::fs::read_to_string(&file).ok()?;
        let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
        return parse_session_json(&v);
    }
    None
}

fn parse_session_json(v: &serde_json::Value) -> Option<LocalSession> {
    let s = |val: Option<&serde_json::Value>| {
        val.and_then(|x| x.as_str())
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
    };
    let auth = v.get("auth")?;
    let account = v.get("account").cloned().unwrap_or(serde_json::Value::Null);
    let access_token = s(auth.get("accessToken"))?;
    let user_id = s(account.get("uid"))?;
    if access_token.is_empty() || user_id.is_empty() {
        return None;
    }
    let expires_at_ms = auth.get("expiresAt").and_then(|x| x.as_i64()).or_else(|| {
        auth.get("expiresAt")
            .and_then(|x| x.as_str())
            .and_then(|x| x.trim().parse::<i64>().ok())
    });
    Some(LocalSession {
        access_token,
        user_id,
        enterprise_id: s(account.get("enterpriseId")).unwrap_or_default(),
        department_info: s(account.get("departmentFullName")).unwrap_or_default(),
        domain: s(auth.get("domain")).unwrap_or_default(),
        expires_at_ms,
    })
}

/// Session usable right now (present + not expired)?
fn session_expired(session: &LocalSession, now_ms: i64) -> bool {
    session
        .expires_at_ms
        .map(|exp| exp <= now_ms + SESSION_EXPIRY_SKEW_MS)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// API response parsing
// ---------------------------------------------------------------------------

fn num(v: Option<&serde_json::Value>) -> Option<f64> {
    match v? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn pick<'a>(obj: &'a serde_json::Value, keys: &[&str]) -> Option<&'a serde_json::Value> {
    keys.iter().find_map(|k| {
        let v = obj.get(k)?;
        match v {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) if s.is_empty() => None,
            _ => Some(v),
        }
    })
}

/// WorkBuddy timestamps are epoch-millis (numeric or numeric string).
fn ts_ms_to_iso(v: Option<&serde_json::Value>) -> Option<String> {
    let n = num(v)?;
    epoch_to_iso(n)
}

/// Parse a datetime string like `"2026-08-31 23:59:59"` (no timezone, treat
/// as UTC) into RFC3339.  Returns `None` on parse failure.
fn parse_dt_str(s: &str) -> Option<String> {
    use chrono::NaiveDateTime;
    let dt = NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    Some(dt.and_utc().to_rfc3339())
}

/// Try epoch-millis first, then fall back to a formatted datetime string
/// (the real API returns `CycleEndTime` as `"YYYY-MM-DD HH:MM:SS"`).
fn parse_expiry(v: Option<&serde_json::Value>) -> Option<String> {
    ts_ms_to_iso(v).or_else(|| v?.as_str().and_then(parse_dt_str))
}

/// Aggregate of the active (Status=0) personal resource packages.
#[derive(Debug, Default, PartialEq)]
struct PersonalUsage {
    used: f64,
    limit: f64,
    remaining: f64,
    /// Per-package sub-items for the frontend's itemized rendering.
    items: Vec<QuotaWindowSubItem>,
}

fn parse_personal_usage(body: &serde_json::Value) -> Result<PersonalUsage, VendorError> {
    // Response nests Accounts under several possible shapes.
    let accounts = body
        .pointer("/data/Response/Data/Accounts")
        .and_then(|v| v.as_array())
        .or_else(|| {
            body.pointer("/data/data/Response/Data/Accounts")
                .and_then(|v| v.as_array())
        })
        .or_else(|| {
            body.pointer("/Response/Data/Accounts")
                .and_then(|v| v.as_array())
        })
        .or_else(|| {
            body.pointer("/data/response/data/accounts")
                .and_then(|v| v.as_array())
        })
        .or_else(|| {
            body.pointer("/response/data/accounts")
                .and_then(|v| v.as_array())
        })
        .ok_or_else(|| VendorError::Parse("no Accounts array".into()))?;

    let mut usage = PersonalUsage::default();
    let mut candidates = 0usize;
    let mut valid = 0usize;

    for resource in accounts {
        let Some(obj) = resource.as_object() else {
            candidates += 1;
            continue;
        };
        let res = serde_json::Value::Object(obj.clone());
        // Status 3 = exhausted/expired package — skip. All other statuses
        // (0=active, 1/2=free/gifted, etc.) are spendable and should be included.
        if let Some(status) = num(pick(&res, &["Status", "status"])) {
            if status == 3.0 {
                continue;
            }
        }
        candidates += 1;
        // API field semantics (confirmed against real data):
        //   CycleCapacityRemainPrecise → 剩余额度
        //   CycleCapacitySizePrecise   → 总量上限
        // 已使用 = total - remaining (不信任 CycleCapacityUsedPrecise)。
        let raw_remaining = num(pick(
            &res,
            &["CycleCapacityRemainPrecise", "cycleCapacityRemainPrecise"],
        ));
        let raw_total = num(pick(
            &res,
            &["CycleCapacitySizePrecise", "cycleCapacitySizePrecise"],
        ));
        let (Some(total), Some(remaining)) = (raw_total, raw_remaining) else {
            continue;
        };
        if total < 0.0 || remaining < 0.0 {
            continue;
        }
        let safe_total = total;
        let safe_remaining = remaining.min(safe_total); // defensive cap
        let safe_used = (safe_total - safe_remaining).max(0.0); // always derived

        let pct = if safe_total > 0.0 {
            safe_used / safe_total * 100.0
        } else {
            0.0
        };
        usage.items.push(QuotaWindowSubItem {
            name: format!("Credits {}", valid + 1),
            used: safe_used,
            total: safe_total,
            pct,
            expires_at: parse_expiry(pick(
                &res,
                &[
                    "CycleEndTime",
                    "cycleEndTime",
                    "expireTime",
                    "expire_time",
                    "expiresAt",
                    "expires_at",
                    "validUntil",
                    "valid_until",
                    "endTime",
                    "end_time",
                ],
            )),
        });
        usage.limit += safe_total;
        usage.remaining += safe_remaining;
        usage.used += safe_used;
        valid += 1;
    }

    // Partial aggregates mislead (an omitted active package would undercount
    // the real balance) — reject instead.
    if candidates > valid {
        return Err(VendorError::Parse(
            "unusable active resource packages".into(),
        ));
    }
    if valid == 0 {
        return Err(VendorError::Empty);
    }
    Ok(usage)
}

/// Enterprise usage: limitNum / credit / cycleResetTime. limitNum = -1 → unlimited.
#[derive(Debug, PartialEq)]
struct EnterpriseUsage {
    used: f64,
    limit: Option<f64>,
    remaining: Option<f64>,
    resets_at: Option<String>,
}

fn parse_enterprise_usage(body: &serde_json::Value) -> Result<EnterpriseUsage, VendorError> {
    let usage = body
        .pointer("/data/data")
        .filter(|v| v.is_object())
        .or_else(|| body.get("data").filter(|v| v.is_object()))
        .or_else(|| body.get("Data").filter(|v| v.is_object()))
        .or_else(|| Some(body).filter(|v| v.is_object()))
        .ok_or_else(|| VendorError::Parse("no enterprise usage object".into()))?;

    let limit_raw = num(pick(usage, &["limitNum", "limit_num"]))
        .ok_or_else(|| VendorError::Parse("no limitNum".into()))?;
    if limit_raw < 0.0 && limit_raw != -1.0 {
        return Err(VendorError::Parse("invalid limitNum".into()));
    }
    let used = num(pick(usage, &["credit", "used", "usedNum", "used_num"]))
        .ok_or_else(|| VendorError::Parse("no usage value".into()))?;
    if used < 0.0 {
        return Err(VendorError::Parse("invalid usage value".into()));
    }
    // remaining derived from limit - used (API returns credit = used).
    let remaining = match limit_raw {
        -1.0 => None,
        limit => Some((limit - used).max(0.0)),
    };
    Ok(EnterpriseUsage {
        used,
        limit: if limit_raw == -1.0 {
            None
        } else {
            Some(limit_raw.max(0.0))
        },
        remaining,
        resets_at: ts_ms_to_iso(pick(usage, &["cycleResetTime", "cycle_reset_time"])),
    })
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

pub trait Http {
    fn post_json(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<String, VendorError>;
}

fn session_headers(session: &LocalSession) -> Vec<(String, String)> {
    let mut h = vec![
        ("Accept".into(), "application/json".into()),
        ("Content-Type".into(), "application/json".into()),
        (
            "Authorization".into(),
            format!("Bearer {}", session.access_token),
        ),
        ("X-User-Id".into(), session.user_id.clone()),
    ];
    if !session.enterprise_id.is_empty() {
        h.push(("X-Enterprise-Id".into(), session.enterprise_id.clone()));
        h.push(("X-Tenant-Id".into(), session.enterprise_id.clone()));
    }
    if !session.domain.is_empty() {
        h.push(("X-Domain".into(), session.domain.clone()));
    }
    if !session.department_info.is_empty() {
        h.push(("X-Department-Info".into(), session.department_info.clone()));
    }
    h
}

struct UreqHttp;
impl Http for UreqHttp {
    fn post_json(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<String, VendorError> {
        let mut req = ureq::post(url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        let resp = req.send_string(body);
        match resp {
            Ok(r) if r.status() == 200 => r
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string())),
            Ok(r) if r.status() == 401 || r.status() == 403 => {
                Err(VendorError::Auth(format!("status code {}", r.status())))
            }
            Ok(r) => Err(VendorError::Api {
                status: r.status(),
                body: r.into_string().unwrap_or_default(),
            }),
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------------

fn fetch_with(http: &dyn Http, session: &LocalSession, now_ms: i64) -> Result<Quota, VendorError> {
    if session_expired(session, now_ms) {
        return Err(VendorError::Auth(
            "WorkBuddy 会话已过期，请重新登录客户端".into(),
        ));
    }
    super::validate_header_safe(&session.access_token)
        .map_err(|_| VendorError::Auth("invalid access token".into()))?;
    super::validate_header_safe(&session.user_id)
        .map_err(|_| VendorError::Auth("invalid user id".into()))?;

    let headers = session_headers(session);

    if session.is_enterprise() {
        let body = http.post_json(&format!("{ENDPOINT}{ENTERPRISE_PATH}"), &headers, "{}")?;
        let v: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| VendorError::Parse(format!("invalid json: {e}")))?;
        check_application_code(&v)?;
        let usage = parse_enterprise_usage(&v)?;
        Ok(build_enterprise_quota(usage))
    } else {
        let request = serde_json::json!({
            "PageNumber": 1,
            "PageSize": 100,
            "ProductCode": PRODUCT_CODE,
            // Request all statuses so free/gifted packages are included.
            "Status": [0, 1, 2],
            "OnlyValidPeriod": true,
        });
        let body = http.post_json(
            &format!("{ENDPOINT}{PERSONAL_PATH}"),
            &headers,
            &request.to_string(),
        )?;
        let v: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| VendorError::Parse(format!("invalid json: {e}")))?;
        check_application_code(&v)?;
        let usage = parse_personal_usage(&v)?;
        Ok(build_personal_quota(usage))
    }
}

/// The API wraps errors as `{"code": N, ...}` with HTTP 200 — surface them.
fn check_application_code(v: &serde_json::Value) -> Result<(), VendorError> {
    if let Some(code) = v.get("code").and_then(|c| c.as_i64()) {
        if code != 0 && code != 200 {
            if code == 401 || code == 403 {
                return Err(VendorError::Auth(format!("application code {code}")));
            }
            return Err(VendorError::Api {
                status: code as u16,
                body: v.to_string(),
            });
        }
    }
    Ok(())
}

fn build_personal_quota(usage: PersonalUsage) -> Quota {
    let used_pct = if usage.limit > 0.0 {
        usage.used / usage.limit * 100.0
    } else {
        0.0
    };
    // Earliest expiry among all sub-items, for the window-level reset display.
    let window_resets_at = usage
        .items
        .iter()
        .filter_map(|i| i.expires_at.as_deref())
        .min();
    Quota {
        vendor: "workbuddy".into(),
        status: QuotaStatus::from_used_pct(used_pct),
        plan_label: Some("Personal".into()),
        windows: vec![QuotaWindow {
            label: "Credits".into(),
            used_pct,
            used_value: Some(usage.used),
            total_value: Some(usage.limit),
            resets_at: window_resets_at.map(|s| s.into()),
            sub_items: Some(usage.items),
            ..Default::default()
        }],
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
        site: None,
    }
}

fn build_enterprise_quota(usage: EnterpriseUsage) -> Quota {
    // Unlimited: no percentage window; show raw usage only.
    let window = match usage.limit {
        None => QuotaWindow {
            label: "Credits".into(),
            used_pct: 0.0,
            used_value: Some(usage.used),
            total_value: None,
            resets_at: usage.resets_at.clone(),
            sub_items: None,
            projected_exhaustion_at: None,
        },
        Some(limit) => {
            let used_pct = if limit > 0.0 {
                usage.used / limit * 100.0
            } else {
                0.0
            };
            QuotaWindow {
                label: "Credits".into(),
                used_pct,
                used_value: Some(usage.used),
                total_value: Some(limit),
                resets_at: usage.resets_at.clone(),
                sub_items: None,
                projected_exhaustion_at: None,
            }
        }
    };
    Quota {
        vendor: "workbuddy".into(),
        status: usage
            .limit
            .map(|l| QuotaStatus::from_used_pct(if l > 0.0 { usage.used / l * 100.0 } else { 0.0 }))
            .unwrap_or(QuotaStatus::Ok),
        plan_label: Some("Enterprise".into()),
        windows: vec![window],
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
        site: None,
    }
}

pub async fn fetch(_credential: &str) -> Result<Quota, VendorError> {
    let session = read_session().ok_or_else(|| {
        VendorError::Auth("未检测到 WorkBuddy 客户端登录，请先登录 WorkBuddy 客户端".into())
    })?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let session = session.clone();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &session, now_ms))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct MockHttp {
        response: Result<String, VendorError>,
        captured_url: std::sync::Mutex<Option<String>>,
    }
    impl MockHttp {
        fn ok(body: &str) -> Self {
            Self {
                response: Ok(body.to_string()),
                captured_url: std::sync::Mutex::new(None),
            }
        }
    }
    impl Http for MockHttp {
        fn post_json(
            &self,
            url: &str,
            _headers: &[(String, String)],
            _body: &str,
        ) -> Result<String, VendorError> {
            *self.captured_url.lock().unwrap() = Some(url.to_string());
            self.response.clone()
        }
    }

    fn session(enterprise_id: &str) -> LocalSession {
        LocalSession {
            access_token: "tok".into(),
            user_id: "u1".into(),
            enterprise_id: enterprise_id.into(),
            department_info: String::new(),
            domain: String::new(),
            expires_at_ms: None,
        }
    }

    fn personal_body() -> serde_json::Value {
        serde_json::json!({
            "code": 0,
            "data": { "Response": { "Data": { "Accounts": [
                // status=0, remain=70, size=100 → used=30, pct=30%
                {
                    "Status": 0,
                    "CycleCapacitySizePrecise": 100,
                    "CycleCapacityRemainPrecise": 70,
                    "CycleCapacityUsedPrecise": 90,
                    "CycleEndTime": 1765900800000i64  // → 2025-12-16T16:00:00Z
                },
                // status=3 (exhausted) → skipped
                {
                    "Status": 3,
                    "CycleCapacitySizePrecise": 50,
                    "CycleCapacityRemainPrecise": 0
                },
                // status=0, remain=150, size=200 → used=50, pct=25%
                {
                    "Status": 0,
                    "CycleCapacitySizePrecise": 200,
                    "CycleCapacityRemainPrecise": "150",
                    "cycleEndTime": 1788220800000i64  // → 2026-09-01T00:00:00Z
                },
                // status=1 (free), remain=10, size=50 → used=40, pct=80%
                {
                    "Status": 1,
                    "CycleCapacitySizePrecise": 50,
                    "CycleCapacityRemainPrecise": 10,
                    "expireTime": 1767225600000i64  // → 2026-01-01T00:00:00Z
                },
                // status=2 (gifted), remain=80, size=100 → used=20, pct=20%
                {
                    "Status": 2,
                    "CycleCapacitySizePrecise": 100,
                    "CycleCapacityRemainPrecise": 80,
                    "expires_at": 1798761600000i64  // 2027-01-01
                },
            ] } } }
        })
    }

    // ── session parsing ──

    #[test]
    fn parse_session_json_minimal() {
        let v = serde_json::json!({
            "auth": {"accessToken": " at ", "domain": "tencent"},
            "account": {"uid": "42", "enterpriseId": "", "departmentFullName": ""}
        });
        let s = parse_session_json(&v).unwrap();
        assert_eq!(s.access_token, "at");
        assert_eq!(s.user_id, "42");
        assert!(s.expires_at_ms.is_none());
        assert!(!s.is_enterprise());
    }

    #[test]
    fn parse_session_json_enterprise_and_expiry() {
        let v = serde_json::json!({
            "auth": {"accessToken": "t", "expiresAt": 1893456000000i64},
            "account": {"uid": "u", "enterpriseId": "ent-9"}
        });
        let s = parse_session_json(&v).unwrap();
        assert!(s.is_enterprise());
        assert_eq!(s.expires_at_ms, Some(1893456000000i64));
    }

    #[test]
    fn parse_session_json_missing_fields_returns_none() {
        assert!(parse_session_json(&serde_json::json!({"auth": {}})).is_none());
        assert!(parse_session_json(&serde_json::json!({})).is_none());
        // uid present but accessToken empty → None
        let v = serde_json::json!({"auth": {"accessToken": ""}, "account": {"uid": "u"}});
        assert!(parse_session_json(&v).is_none());
    }

    #[test]
    fn session_expired_respects_skew() {
        let mut s = session("");
        s.expires_at_ms = Some(1000);
        // now within [1000-skew, ∞) → expired; just before the skew window → not.
        assert!(session_expired(&s, 1000 - SESSION_EXPIRY_SKEW_MS));
        assert!(!session_expired(&s, 1000 - SESSION_EXPIRY_SKEW_MS - 1));
        assert!(session_expired(&s, 1000));
        s.expires_at_ms = None;
        assert!(!session_expired(&s, i64::MAX / 2)); // no expiry recorded
    }

    // ── personal parsing ──

    #[test]
    fn parse_personal_aggregates_active_packages() {
        let usage = parse_personal_usage(&personal_body()).unwrap();
        // Correct semantics: CycleCapacityRemainPrecise = remaining,
        // CycleCapacitySizePrecise = total. used = total - remaining.
        assert_eq!(usage.limit, 450.0); // 100+200+50+100
        assert_eq!(usage.used, 140.0); // (100-70)+(200-150)+(50-10)+(100-80)
        assert_eq!(usage.remaining, 310.0); // 70+150+10+80
        assert_eq!(usage.items.len(), 4);
        // Expiry parsed from various field name variants.
        assert_eq!(
            usage.items[0].expires_at,
            Some("2025-12-16T16:00:00+00:00".into())
        );
        assert_eq!(
            usage.items[1].expires_at,
            Some("2026-09-01T00:00:00+00:00".into())
        );
        assert_eq!(
            usage.items[2].expires_at,
            Some("2026-01-01T00:00:00+00:00".into())
        ); // expireTime
        assert_eq!(
            usage.items[3].expires_at,
            Some("2027-01-01T00:00:00+00:00".into())
        );
    }

    #[test]
    fn parse_personal_rejects_partial_active() {
        // An active package with missing capacity fields → candidates > valid.
        let v = serde_json::json!({
            "data": { "response": { "data": { "accounts": [
                {"Status": 0, "CycleCapacitySizePrecise": 100, "CycleCapacityRemainPrecise": 50},
                {"Status": 0}
            ] } } }
        });
        assert!(matches!(
            parse_personal_usage(&v),
            Err(VendorError::Parse(_))
        ));
    }

    #[test]
    fn parse_personal_includes_nonzero_status_excludes_exhausted() {
        // Status 1 (free) and 2 (gifted) are included; only 3 (exhausted) is skipped.
        // Correct field semantics: RemainPrecise = remaining, SizePrecise = total.
        let v = serde_json::json!({
            "data": { "Response": { "Data": { "Accounts": [
                {"Status": 1, "CycleCapacitySizePrecise": 10, "CycleCapacityRemainPrecise": 10},
                {"Status": 2, "CycleCapacitySizePrecise": 20, "CycleCapacityRemainPrecise": 15},
                {"Status": 3, "CycleCapacitySizePrecise": 5,  "CycleCapacityRemainPrecise": 0},
                {"Status": 0, "CycleCapacitySizePrecise": 30, "CycleCapacityRemainPrecise": 25},
            ] } } }
        });
        let usage = parse_personal_usage(&v).unwrap();
        // used = total - remaining for each package
        assert_eq!(usage.limit, 60.0); // 10+20+30 (status 3 excluded)
        assert_eq!(usage.used, 10.0); // (10-10)+(20-15)+(30-25)
        assert_eq!(usage.remaining, 50.0); // 10+15+25
        assert_eq!(usage.items.len(), 3);
    }

    #[test]
    fn parse_personal_remaining_always_derived_not_trusted() {
        // CycleCapacityUsedPrecise claims 90 used, but the correct used
        // is total - remaining (RemainPrecise = remaining = 80).
        // The adapter must ignore the API's UsedPrecise field.
        let v = serde_json::json!({
            "Response": { "Data": { "Accounts": [
                {"Status": 0, "CycleCapacitySizePrecise": 100,
                 "CycleCapacityRemainPrecise": 80,   // remaining = 80
                 "CycleCapacityUsedPrecise": 90},    // API lies: says used=90
            ] } }
        });
        let usage = parse_personal_usage(&v).unwrap();
        assert_eq!(usage.used, 20.0); // 100 - 80, NOT 90
        assert_eq!(usage.remaining, 80.0); // from RemainPrecise
        assert_eq!(usage.items[0].pct, 20.0); // used/total = 20/100
    }

    #[test]
    fn parse_expiry_handles_datetime_string_and_epoch_millis() {
        // Real API returns CycleEndTime as "YYYY-MM-DD HH:MM:SS" string.
        assert_eq!(
            parse_expiry(Some(&serde_json::Value::String(
                "2026-08-31 23:59:59".into()
            ))),
            Some("2026-08-31T23:59:59+00:00".into())
        );
        // Epoch millis still work (backward compat).
        assert_eq!(
            parse_expiry(Some(&serde_json::Value::String("1765900800000".into()))),
            Some("2025-12-16T16:00:00+00:00".into())
        );
        // Numeric epoch millis.
        assert_eq!(
            parse_expiry(Some(&serde_json::json!(1788220800000i64))),
            Some("2026-09-01T00:00:00+00:00".into())
        );
        // Null / empty → None.
        assert!(parse_expiry(Some(&serde_json::Value::Null)).is_none());
        assert!(parse_expiry(Some(&serde_json::Value::String("".into()))).is_none());
    }

    #[test]
    fn parse_personal_empty_accounts_is_empty_error() {
        let v = serde_json::json!({"Response": {"Data": {"Accounts": []}}});
        assert!(matches!(parse_personal_usage(&v), Err(VendorError::Empty)));
    }

    #[test]
    fn parse_personal_no_accounts_is_parse_error() {
        assert!(matches!(
            parse_personal_usage(&serde_json::json!({"data": {}})),
            Err(VendorError::Parse(_))
        ));
    }

    // ── enterprise parsing ──

    #[test]
    fn parse_enterprise_finite_limit() {
        let v = serde_json::json!({
            "data": {"data": {"limitNum": 500, "credit": 120, "cycleResetTime": 1893456000000i64}}
        });
        let u = parse_enterprise_usage(&v).unwrap();
        assert_eq!(u.limit, Some(500.0));
        assert_eq!(u.used, 120.0);
        assert!(u.resets_at.is_some());
    }

    #[test]
    fn parse_enterprise_unlimited() {
        let v = serde_json::json!({"data": {"limitNum": -1, "credit": 99}});
        let u = parse_enterprise_usage(&v).unwrap();
        assert_eq!(u.limit, None);
        assert_eq!(u.used, 99.0);
    }

    #[test]
    fn parse_enterprise_missing_fields_errors() {
        assert!(parse_enterprise_usage(&serde_json::json!({"data": {"credit": 1}})).is_err());
        assert!(parse_enterprise_usage(&serde_json::json!({"data": {"limitNum": 10}})).is_err());
    }

    #[test]
    fn parse_enterprise_invalid_limit_rejected() {
        let v = serde_json::json!({"data": {"limitNum": -5, "credit": 1}});
        assert!(parse_enterprise_usage(&v).is_err());
    }

    // ── application code ──

    #[test]
    fn check_application_code_passes_ok_variants() {
        assert!(check_application_code(&serde_json::json!({"code": 0})).is_ok());
        assert!(check_application_code(&serde_json::json!({"code": 200})).is_ok());
        assert!(check_application_code(&serde_json::json!({"data": 1})).is_ok());
        // no code
    }

    #[test]
    fn check_application_code_auth_and_error() {
        assert!(matches!(
            check_application_code(&serde_json::json!({"code": 401})),
            Err(VendorError::Auth(_))
        ));
        assert!(matches!(
            check_application_code(&serde_json::json!({"code": 500})),
            Err(VendorError::Api { .. })
        ));
    }

    // ── fetch_with end-to-end (mocked HTTP) ──

    #[test]
    fn fetch_with_personal_builds_quota() {
        let http = MockHttp::ok(&personal_body().to_string());
        let q = fetch_with(&http, &session(""), 0).unwrap();
        assert_eq!(q.vendor, "workbuddy");
        assert_eq!(q.plan_label.as_deref(), Some("Personal"));
        assert_eq!(q.windows.len(), 1);
        let w = &q.windows[0];
        assert_eq!(w.label, "Credits");
        // 4 packages: 100+200+50+100 = 450 total; used = (100-70)+(200-150)+(50-10)+(100-80) = 140
        assert_eq!(w.total_value, Some(450.0));
        assert_eq!(w.used_value, Some(140.0));
        assert!((w.used_pct - 31.111).abs() < 0.01);
        assert!(w.sub_items.is_some());
        assert_eq!(w.sub_items.as_ref().unwrap().len(), 4);
        // Expiry times parsed from various field name variants.
        let items = w.sub_items.as_ref().unwrap();
        assert_eq!(
            items[0].expires_at,
            Some("2025-12-16T16:00:00+00:00".into())
        ); // CycleEndTime
        assert_eq!(
            items[1].expires_at,
            Some("2026-09-01T00:00:00+00:00".into())
        ); // cycleEndTime
        assert_eq!(
            items[2].expires_at,
            Some("2026-01-01T00:00:00+00:00".into())
        ); // expireTime // expireTime
        assert_eq!(
            items[3].expires_at,
            Some("2027-01-01T00:00:00+00:00".into())
        ); // expires_at
           // Window-level resets_at = earliest sub-item expiry.
        assert_eq!(w.resets_at, Some("2025-12-16T16:00:00+00:00".into()));
        assert!(http
            .captured_url
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains(PERSONAL_PATH));
    }

    #[test]
    fn fetch_with_enterprise_uses_enterprise_path() {
        let body = serde_json::json!({
            "code": 0,
            "data": {"data": {"limitNum": 1000, "credit": 400, "cycleResetTime": 1893456000000i64}}
        });
        let http = MockHttp::ok(&body.to_string());
        let q = fetch_with(&http, &session("ent-1"), 0).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("Enterprise"));
        assert_eq!(q.windows[0].used_pct, 40.0);
        assert!(http
            .captured_url
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains(ENTERPRISE_PATH));
    }

    #[test]
    fn fetch_with_expired_session_is_auth_error() {
        let http = MockHttp::ok("{}");
        let mut s = session("");
        s.expires_at_ms = Some(100);
        let err = fetch_with(&http, &s, 200).unwrap_err();
        assert!(matches!(err, VendorError::Auth(_)));
    }

    #[test]
    fn fetch_with_http_auth_error_propagates() {
        let http = MockHttp {
            response: Err(VendorError::Auth("401".into())),
            captured_url: std::sync::Mutex::new(None),
        };
        assert!(matches!(
            fetch_with(&http, &session(""), 0),
            Err(VendorError::Auth(_))
        ));
    }

    // ── quota builders ──

    #[test]
    fn build_personal_quota_single_package_has_subitems() {
        let usage = PersonalUsage {
            used: 10.0,
            limit: 100.0,
            remaining: 90.0,
            items: vec![QuotaWindowSubItem {
                name: "Credits 1".into(),
                used: 10.0,
                total: 100.0,
                pct: 10.0,
                expires_at: Some("2026-01-01T00:00:00+00:00".into()),
            }],
        };
        let q = build_personal_quota(usage);
        // Single-package now shows sub-items (and its expiry) — consistent with Qoder.
        assert!(q.windows[0].sub_items.is_some());
        assert_eq!(q.windows[0].sub_items.as_ref().unwrap().len(), 1);
        assert_eq!(
            q.windows[0].sub_items.as_ref().unwrap()[0].expires_at,
            Some("2026-01-01T00:00:00+00:00".into())
        );
        // Window-level resets_at mirrors the earliest (and only) sub-item expiry.
        assert_eq!(
            q.windows[0].resets_at,
            Some("2026-01-01T00:00:00+00:00".into())
        );
        assert_eq!(q.status, QuotaStatus::Ok);
    }

    #[test]
    fn build_enterprise_unlimited_status_ok() {
        let q = build_enterprise_quota(EnterpriseUsage {
            used: 999.0,
            limit: None,
            remaining: None,
            resets_at: None,
        });
        assert_eq!(q.status, QuotaStatus::Ok);
        assert_eq!(q.windows[0].total_value, None);
        assert_eq!(q.windows[0].used_pct, 0.0);
    }
}
