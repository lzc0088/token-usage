//! Volcengine Ark Coding Plan adapter.
//!
//! Two credential modes (token-monitor volcengineLimits.js):
//! 1. **AK+SK** (Access Key `AKLT…` + Secret): HMAC-SHA256 signed POST to
//!    `GetCodingPlanUsage` → `Result.QuotaUsage[]` with three windows
//!    (5-hour / weekly / monthly), each carrying a real `ResetTimestamp`.
//! 2. **Ark API key** (`ark-…`): Bearer token POST to GetCodingPlanUsage.
//!    The canonical token-monitor path probes chat/completions and reads
//!    `x-ratelimit-*` headers — a separate follow-up will switch to that
//!    richer path.

use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::types::{epoch_to_iso, parse_iso, Quota, QuotaStatus, QuotaWindow};
use super::VendorError;

type HmacSha256 = Hmac<Sha256>;

const URL: &str = "https://open.volcengineapi.com/?Action=GetCodingPlanUsage&Version=2024-01-01";
const HOST: &str = "open.volcengineapi.com";
const REGION: &str = "cn-beijing";
const SERVICE: &str = "ark";
const CONTENT_TYPE: &str = "application/x-www-form-urlencoded; charset=utf-8";
const SIGNED_HEADERS: &str = "content-type;host;x-content-sha256;x-date";
/// Console-panel Cookie API for subscription-trade info (Coding Plan expiry).
/// Requires the browser Cookie + matching csrfToken.
const SUBSCRIBE_TRADE_URL: &str =
    "https://console.volcengine.com/api/top/ark/cn-beijing/2024-01-01/ListSubscribeTrade";

// Ark API key probe (token-monitor volcengineLimits.js fetchVolcengineArkLimits).
const ARK_PROBE_MODELS: &[&str] = &[
    "doubao-seed-2.0-code",
    "doubao-1.5-pro-32k",
    "doubao-lite-32k",
];

#[derive(Debug, Deserialize)]
struct Credential {
    key: String,
    #[serde(default)]
    secret: String,
    #[serde(default)]
    region: Option<String>,
    /// Optional console-panel cookie for fetching Coding Plan subscription
    /// expiry (`ListSubscribeTrade`). When present, the plan window's
    /// `resets_at` is overwritten with the real `EndTime`.
    #[serde(default)]
    cookie: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Resp {
    #[serde(default, rename = "Result", alias = "result")]
    result: RespResult,
}

#[derive(Default, Debug, Deserialize)]
#[allow(non_snake_case)]
struct RespResult {
    #[serde(default, rename = "QuotaUsage")]
    quota_usage: Vec<QuotaEntry>,
    #[serde(default)]
    PlanName: Option<String>,
    #[serde(default)]
    planName: Option<String>,
    #[serde(default)]
    PlanTier: Option<String>,
    #[serde(default)]
    planTier: Option<String>,
    #[serde(default)]
    ProductName: Option<String>,
    #[serde(default)]
    productName: Option<String>,
    #[serde(default)]
    PackageName: Option<String>,
    #[serde(default)]
    packageName: Option<String>,
}
#[derive(Debug, Deserialize)]
struct QuotaEntry {
    #[serde(default, rename = "Level")]
    level_camel: Option<String>,
    #[serde(default)]
    level: Option<String>,
    #[serde(default, rename = "Percent")]
    percent_camel: Option<f64>,
    #[serde(default)]
    percent: Option<f64>,
    /// Real reset marker (epoch seconds/ms). token-monitor: ResetTimestamp.
    #[serde(default, rename = "ResetTimestamp")]
    reset_ts_camel: Option<f64>,
    #[serde(default, rename = "resetTimestamp", alias = "reset_time")]
    reset_ts: Option<f64>,
}

pub trait Http {
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<String, VendorError>;

    /// Like `post`, but also returns the response headers. Default impl returns
    /// an empty header vec — volcengine overrides this for Ark probe mode.
    fn post_with_response_headers(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<(String, Vec<(String, String)>), VendorError> {
        self.post(url, headers, body).map(|b| (b, vec![]))
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex(&h.finalize())
}
fn hmac_bytes(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut m = <HmacSha256 as Mac>::new_from_slice(key).expect("hmac accepts any key length");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

/// Format volcengine plan text (token-monitor displayPlanText).
fn volcengine_display_plan(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    // Strip leading PLAN_TIER_ prefix (case-insensitive, matching JS /^PLAN_TIER_/i).
    let s = if s.len() >= 10 && s[..10].eq_ignore_ascii_case("PLAN_TIER_") {
        &s[10..]
    } else {
        s
    };
    // Replace separators, collapse whitespace, title-case each word.
    s.replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .map(|word| {
            let lower = word.to_lowercase();
            if lower == "ai" {
                "AI".to_string()
            } else {
                let mut chars = word.chars();
                match chars.next() {
                    Some(c) => {
                        let mut r = String::with_capacity(word.len());
                        r.push(c.to_ascii_uppercase());
                        r.push_str(chars.as_str());
                        r
                    }
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract plan label from GetCodingPlanUsage response Result fields.
fn volcengine_plan_label(result: &RespResult) -> Option<String> {
    for v in [
        &result.PlanName,
        &result.planName,
        &result.PlanTier,
        &result.planTier,
        &result.ProductName,
        &result.productName,
        &result.PackageName,
        &result.packageName,
    ]
    .into_iter()
    .flatten()
    {
        if !v.is_empty() {
            let formatted = volcengine_display_plan(v);
            if !formatted.is_empty() {
                return Some(formatted);
            }
        }
    }
    None
}

fn level_label(level: &str) -> &'static str {
    let l = level.to_ascii_lowercase();
    if l.contains('5') || l.contains("session") || l.contains("hour") {
        "5h"
    } else if l.contains("week") {
        "周"
    } else if l.contains("month") {
        "月"
    } else {
        "额度"
    }
}

/// Parse the GetCodingPlanUsage response into all windows (5h / weekly / monthly).
pub fn parse(body: &str) -> Result<Quota, VendorError> {
    let resp: Resp = serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    let mut windows: Vec<QuotaWindow> = Vec::new();
    for q in &resp.result.quota_usage {
        let level = q
            .level_camel
            .as_deref()
            .or(q.level.as_deref())
            .unwrap_or("");
        let pct = match q.percent_camel.or(q.percent) {
            Some(p) => p,
            None => continue,
        };
        let label = level_label(level);
        let resets_at = q.reset_ts_camel.or(q.reset_ts).and_then(epoch_to_iso);
        windows.push(QuotaWindow {
            label: label.into(),
            used_pct: pct.clamp(0.0, 100.0),
            resets_at,
            ..Default::default()
        });
    }
    if windows.is_empty() {
        return Err(VendorError::Empty);
    }
    let status = QuotaStatus::worst_of(
        windows
            .iter()
            .map(|w| QuotaStatus::from_used_pct(w.used_pct)),
    );
    Ok(Quota {
        vendor: "volcengine".into(),
        plan_label: volcengine_plan_label(&resp.result).or(Some("Coding Plan".into())),
        status,
        windows,
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
    })
}

// ── Ark API key probe (chat/completions → x-ratelimit-* headers) ────────

/// Parse `x-ratelimit-reset-requests` into an RFC3339 string.
/// Handles epoch seconds/ms, ISO dates, relative durations ("1h30m", "30s"),
/// or plain seconds. Ported from token-monitor `resetHeaderToIso`.
fn parse_ratelimit_reset(raw: &str, now: i64) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Absolute epoch (seconds or ms).
    if let Ok(n) = trimmed.parse::<f64>() {
        return if n > 0.0 { epoch_to_iso(n) } else { None };
    }
    // ISO date.
    if let Some(iso) = parse_iso(trimmed) {
        return Some(iso);
    }
    // Relative duration: "1h30m", "5m", "30s", "2d4h", etc.
    // Hand-rolled parser — no regex dependency.
    let lower = trimmed.to_lowercase();
    let mut seconds: i64 = 0;
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Skip non-digit characters.
        if !chars[i].is_ascii_digit() && chars[i] != '.' {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
        }
        let num_str: String = chars[start..i].iter().collect();
        let amount: f64 = match num_str.parse::<f64>() {
            Ok(v) if v > 0.0 && v.is_finite() => v,
            _ => {
                i += 1;
                continue;
            }
        };
        if i < chars.len() {
            let unit = chars[i];
            seconds += match unit {
                'd' => (amount * 86400.0) as i64,
                'h' => (amount * 3600.0) as i64,
                'm' => (amount * 60.0) as i64,
                's' => amount as i64,
                _ => 0,
            };
            i += 1;
        }
    }
    if seconds > 0 {
        let target_ms = now
            .checked_mul(1000)?
            .checked_add(seconds.checked_mul(1000)?)?;
        return chrono::DateTime::<chrono::Utc>::from_timestamp_millis(target_ms)
            .map(|dt| dt.to_rfc3339());
    }
    None
}

/// Parse Ark probe response (headers + optional body) into windows.
/// Mirrors token-monitor `parseVolcengineArkUsage`: reads x-ratelimit-*
/// headers first; falls back to `body.usage.total_tokens`/`totalTokens`.
fn parse_ark_response(
    response_headers: &[(String, String)],
    body_str: &str,
    now: i64,
) -> Vec<QuotaWindow> {
    let mut remaining: Option<f64> = None;
    let mut limit: Option<f64> = None;
    let mut reset_str: Option<String> = None;
    for (k, v) in response_headers {
        match k.to_lowercase().as_str() {
            "x-ratelimit-remaining-requests" => {
                remaining = v.parse::<f64>().ok().filter(|r| r.is_finite());
            }
            "x-ratelimit-limit-requests" => {
                limit = v.parse::<f64>().ok().filter(|l| l.is_finite() && *l > 0.0);
            }
            "x-ratelimit-reset-requests" => {
                reset_str = Some(v.clone());
            }
            _ => {}
        }
    }
    let resets_at = reset_str.and_then(|s| parse_ratelimit_reset(&s, now));

    // Primary path: ratelimit headers.
    if let (Some(r), Some(l)) = (remaining, limit) {
        let used_pct = ((l - r) / l * 100.0).clamp(0.0, 100.0);
        return vec![QuotaWindow {
            label: "Ark".into(),
            used_pct,
            resets_at,
            ..Default::default()
        }];
    }

    // Fallback: response body `usage.total_tokens` / `totalTokens`.
    if let Ok(body) = serde_json::from_str::<serde_json::Value>(body_str) {
        let total_tokens = body
            .get("usage")
            .and_then(|u| {
                u.get("total_tokens")
                    .or_else(|| u.get("totalTokens"))
                    .and_then(|v| v.as_f64())
            })
            .filter(|t| t.is_finite());
        if total_tokens.is_some() {
            return vec![QuotaWindow {
                label: "Ark".into(),
                used_pct: 0.0, // Token-only probe (no limit without ratelimit headers).
                resets_at,
                ..Default::default()
            }];
        }
    }

    vec![]
}

/// Format the Volcengine Ark chat/completions URL for a given region.
/// Validate region identifier — only `[a-zA-Z0-9-]+` (standard cloud region format).
fn validate_region(region: &str) -> Result<(), VendorError> {
    if region.is_empty()
        || !region
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(VendorError::Parse(format!("invalid region: {region}")));
    }
    Ok(())
}

fn ark_chat_url(region: &str) -> String {
    format!("https://ark.{region}.volces.com/api/coding/v3/chat/completions")
}

/// Probe the Ark API key via chat/completions and extract ratelimit windows.
/// Ported from token-monitor `fetchVolcengineArkLimits` + `probeVolcengineArkModel`.
fn fetch_ark_limits(http: &dyn Http, api_key: &str, region: &str) -> Result<Quota, VendorError> {
    let now = chrono::Utc::now().timestamp();
    let mut last_error: Option<VendorError> = None;

    for model in ARK_PROBE_MODELS {
        let probe_body = serde_json::json!({
            "model": model,
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "hi"}]
        })
        .to_string();
        let headers = vec![
            ("Accept".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {api_key}")),
            ("Content-Type".into(), "application/json".into()),
        ];

        // Always call — headers are returned even on HTTP errors.
        let chat_url = ark_chat_url(region);
        let (body_str, resp_headers) =
            match http.post_with_response_headers(&chat_url, &headers, &probe_body) {
                Ok(v) => v,
                Err(e) => {
                    // Network/tls/timeout — try next model.
                    last_error = Some(e);
                    continue;
                }
            };

        // Check for 401 from the body or error (unauthorized → immediate failure).
        let body_lower = body_str.to_lowercase();
        if body_lower.contains("\"error\"")
            && (body_lower.contains("auth")
                || body_lower.contains("401")
                || body_lower.contains("unauthorized"))
        {
            return Err(VendorError::Parse(
                "Volcengine Ark: unauthorized (check API key)".into(),
            ));
        }

        let windows = parse_ark_response(&resp_headers, &body_str, now);
        if !windows.is_empty() {
            let status = QuotaStatus::worst_of(
                windows
                    .iter()
                    .map(|w| QuotaStatus::from_used_pct(w.used_pct)),
            );
            return Ok(Quota {
                vendor: "volcengine".into(),
                plan_label: Some("Ark API".into()),
                status,
                windows,
                balance: None,
                refreshed_at: None,
                error: None,
                cookie_error: None,
                expires_at: None,
            });
        }

        // No windows from this model — try the next.
        last_error = Some(VendorError::Empty);
    }

    Err(last_error.unwrap_or(VendorError::Empty))
}

/// Build the signed POST headers for GetCodingPlanUsage. Pure (time injected).
fn signed_headers(
    ak: &str,
    sk: &str,
    timestamp: &str,
    date: &str,
    region: &str,
) -> Vec<(String, String)> {
    let payload_hash = sha256_hex(b"");
    let canonical = format!(
        "POST\n/\nAction=GetCodingPlanUsage&Version=2024-01-01\n\
         content-type:{CONTENT_TYPE}\nhost:{HOST}\nx-content-sha256:{payload_hash}\nx-date:{timestamp}\n\n\
         {SIGNED_HEADERS}\n{payload_hash}"
    );
    let credential_scope = format!("{date}/{region}/{SERVICE}/request");
    let string_to_sign = format!(
        "HMAC-SHA256\n{timestamp}\n{credential_scope}\n{}",
        sha256_hex(canonical.as_bytes())
    );
    let date_key = hmac_bytes(sk.as_bytes(), date.as_bytes());
    let region_key = hmac_bytes(&date_key, region.as_bytes());
    let service_key = hmac_bytes(&region_key, SERVICE.as_bytes());
    let signing_key = hmac_bytes(&service_key, b"request");
    let signature = hex(&hmac_bytes(&signing_key, string_to_sign.as_bytes()));
    let auth = format!(
        "HMAC-SHA256 Credential={ak}/{credential_scope}, SignedHeaders={SIGNED_HEADERS}, Signature={signature}"
    );
    vec![
        ("Accept".into(), "application/json".into()),
        ("Content-Type".into(), CONTENT_TYPE.into()),
        ("Host".into(), HOST.into()),
        ("X-Date".into(), timestamp.into()),
        ("X-Content-Sha256".into(), payload_hash),
        ("Authorization".into(), auth),
    ]
}

pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let cred: Credential =
        serde_json::from_str(credential).map_err(|e| VendorError::Parse(e.to_string()))?;
    super::validate_header_safe(&cred.key)?;

    let region = cred.region.as_deref().unwrap_or(REGION);
    validate_region(region)?;

    let mut q = if cred.key.starts_with("ark-") {
        // Ark API Key: try Bearer on GetCodingPlanUsage first (may return full
        // Coding Plan windows like AK+SK), then fall back to chat probe.
        match try_bearer_coding_plan(http, &cred.key) {
            Ok(q) => q,
            Err(e) => {
                eprintln!(
                    "[quota] volcengine Ark Bearer→CodingPlan: {e}, falling back to chat probe"
                );
                fetch_ark_limits(http, &cred.key, region)?
            }
        }
    } else {
        // AK+SK: HMAC-SHA256 signed request.
        if cred.secret.is_empty() {
            return Err(VendorError::Parse(
                "AK+SK needs a secret; enter both AK (AKLT…) and Secret".into(),
            ));
        }
        let now = chrono::Utc::now();
        let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();
        let headers = signed_headers(&cred.key, &cred.secret, &timestamp, &date, region);
        let body = http.post(URL, &headers, "")?;
        parse(&body)?
    };

    // Optional: overlay real subscription plan name + end date if the user
    // pasted a console cookie. Silently ignored on failure — usage data still
    // lands. Without a cookie, plan_label stays "Coding Plan" and resets_at
    // stays the per-window ResetTimestamp.
    if let Some(raw_cookie) = cred.cookie.as_deref() {
        let cookie = raw_cookie.trim();
        if !cookie.is_empty() {
            match fetch_subscription_info(http, cookie) {
                Ok(Some(info)) => {
                    if let Some(ref end) = info.end_time {
                        apply_subscription_end(&mut q, end);
                    }
                    if let Some(ref plan) = info.plan_label {
                        q.plan_label = Some(plan.clone());
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    // Cookie expired/incomplete → set `cookie_error` so the
                    // frontend shows an inline "更新 Cookie" hint at the
                    // expiry slot. Usage windows (from the API Key path) are
                    // unaffected and keep rendering.
                    let msg = e.to_string();
                    let is_cookie_problem =
                        super::is_auth_error(&e) || msg.contains("csrf") || msg.contains("Cookie");
                    if is_cookie_problem {
                        q.cookie_error = Some("Cookie 已过期，套餐到期信息暂未显示".into());
                    } else {
                        eprintln!("[quota] volcengine subscription-info: {e}");
                    }
                }
            }
        }
    }
    Ok(q)
}

/// Extract the `csrfToken` value from a raw cookie string. Cookie API requires
/// both the cookie and a matching CSRF header.
fn extract_csrf(cookie: &str) -> Option<String> {
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("csrfToken=") {
            return Some(rest.to_string());
        }
    }
    None
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct SubscribeTradeResp {
    #[serde(default)]
    Result: Option<SubscribeTradeResult>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct SubscribeTradeResult {
    #[serde(default)]
    InfoList: Vec<SubscribeTradeInfo>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct SubscribeTradeInfo {
    #[serde(default)]
    Status: Option<String>,
    #[serde(default)]
    EndTime: Option<String>,
    /// 套餐档位标识："lite" / "pro" 等（ListSubscribeTrade 返回）。
    #[serde(default)]
    BizInfo: Option<String>,
    /// 容错：部分返回可能改用这些字段名携带套餐名。
    #[serde(default)]
    ProductName: Option<String>,
    #[serde(default)]
    PlanName: Option<String>,
}

/// 从 ListSubscribeTrade 提取的订阅信息：套餐显示名 + 到期时间。
#[derive(Debug, Default)]
struct SubscribeInfo {
    plan_label: Option<String>,
    end_time: Option<String>,
}

/// 把套餐档位（"lite"/"pro"）格式化成显示名，如 "Coding Plan Lite"。
/// 字段优先级：BizInfo → ProductName → PlanName。
fn format_plan_from_info(info: &SubscribeTradeInfo) -> Option<String> {
    let raw = info
        .BizInfo
        .as_deref()
        .or(info.ProductName.as_deref())
        .or(info.PlanName.as_deref())?;
    let tier = volcengine_display_plan(raw);
    if tier.is_empty() {
        return None;
    }
    Some(format!("Coding Plan {tier}"))
}

/// Call `ListSubscribeTrade` with the console cookie + csrf. Returns the
/// plan label + `EndTime` of the running Coding Plan.
fn fetch_subscription_info(
    http: &dyn Http,
    cookie: &str,
) -> Result<Option<SubscribeInfo>, VendorError> {
    let csrf = extract_csrf(cookie).ok_or_else(|| {
        VendorError::Parse("Cookie 中缺少 csrfToken（未完整复制控制台 Cookie）".into())
    })?;
    let headers = vec![
        ("Accept".into(), "application/json, text/plain, */*".into()),
        ("Content-Type".into(), "application/json".into()),
        ("Cookie".into(), cookie.into()),
        ("Origin".into(), "https://console.volcengine.com".into()),
        (
            "Referer".into(),
            "https://console.volcengine.com/ark/region:cn-beijing/subscription/coding-plan".into(),
        ),
        ("X-Csrf-Token".into(), csrf),
        (
            "User-Agent".into(),
            "Mozilla/5.0 AppleWebKit/537.36 Chrome/150 Safari/537.36".into(),
        ),
    ];
    let body_str =
        r#"{"ResourceTypes":["CodingPlan"],"ResourceNames":[""],"BizInfos":["lite","pro"]}"#;
    let body = http.post(SUBSCRIBE_TRADE_URL, &headers, body_str)?;
    parse_subscribe_info(&body)
}

fn parse_subscribe_info(body: &str) -> Result<Option<SubscribeInfo>, VendorError> {
    let resp: SubscribeTradeResp = serde_json::from_str(body)
        .map_err(|e| VendorError::Parse(format!("volcengine subscribe: {e}")))?;
    let list = match resp.Result {
        Some(r) => r.InfoList,
        None => return Ok(None),
    };
    // Prefer a Running entry; fall back to the first record.
    let picked = list
        .iter()
        .find(|i| i.Status.as_deref() == Some("Running"))
        .or_else(|| list.first());
    Ok(picked.map(|i| SubscribeInfo {
        plan_label: format_plan_from_info(i),
        end_time: i.EndTime.clone(),
    }))
}

/// Set the plan expiry (from the console cookie's ListSubscribeTrade EndTime)
/// on `expires_at`. Per-window `resets_at` (5h/周/月 rolling resets) is left
/// untouched so both the reset countdown and the plan "到期" tag are correct.
fn apply_subscription_end(q: &mut Quota, end_iso: &str) {
    q.expires_at = Some(end_iso.to_string());
}

/// Try Bearer auth on GetCodingPlanUsage (Ark API key may support this).
fn try_bearer_coding_plan(http: &dyn Http, api_key: &str) -> Result<Quota, VendorError> {
    let headers = vec![
        ("Accept".into(), "application/json".into()),
        ("Authorization".into(), format!("Bearer {api_key}")),
    ];
    let body = http.post(URL, &headers, "")?;
    let mut q = parse(&body)?;
    // Bearer via Ark key → tag with "(Ark)" to distinguish from AK+SK which
    // may return the same plan name.
    let plan = q.plan_label.take().unwrap_or_else(|| "Coding Plan".into());
    q.plan_label = Some(format!("{plan} (Ark)"));
    Ok(q)
}

pub async fn fetch(credential: &str) -> Result<Quota, VendorError> {
    let cred = credential.to_string();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &cred))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

struct UreqHttp;
impl Http for UreqHttp {
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<String, VendorError> {
        let mut req = ureq::post(url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        let resp = req
            .send_string(body)
            .map_err(|e| VendorError::Network(e.to_string()))?;
        resp.into_string()
            .map_err(|e| VendorError::Network(e.to_string()))
    }

    fn post_with_response_headers(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<(String, Vec<(String, String)>), VendorError> {
        let mut req = ureq::post(url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        match req.send_string(body) {
            // Success (2xx): extract body + headers normally.
            Ok(resp) => {
                let resp_headers = extract_headers(&resp);
                let body_str = resp
                    .into_string()
                    .map_err(|e| VendorError::Network(e.to_string()))?;
                Ok((body_str, resp_headers))
            }
            // Non-2xx: ureq Error wraps the response. Extract body + headers from it.
            Err(ureq::Error::Status(_code, resp)) => {
                let resp_headers = extract_headers(&resp);
                let body_str = resp
                    .into_string()
                    .map_err(|e| VendorError::Network(e.to_string()))?;
                Ok((body_str, resp_headers))
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

fn extract_headers(resp: &ureq::Response) -> Vec<(String, String)> {
    resp.headers_names()
        .iter()
        .map(|name| {
            let value = resp.header(name).unwrap_or("");
            (name.to_string(), value.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_collects_all_windows_with_reset() {
        let body = r#"{"Result":{"QuotaUsage":[
            {"Level":"5-hour","Percent":30.0,"ResetTimestamp":1893456000},
            {"Level":"weekly","Percent":78.0},
            {"Level":"monthly","Percent":50.0}
        ]}}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.vendor, "volcengine");
        assert_eq!(q.windows.len(), 3);
        let labels: Vec<&str> = q.windows.iter().map(|w| w.label.as_str()).collect();
        assert!(labels.contains(&"5h"));
        assert!(labels.contains(&"周"));
        assert!(labels.contains(&"月"));
        // 5h window carries the parsed reset timestamp
        let five = q.windows.iter().find(|w| w.label == "5h").unwrap();
        assert!(five.resets_at.as_deref().unwrap().starts_with("2030"));
        // worst (weekly 78%) → Low
        assert_eq!(q.status, QuotaStatus::Low);
        assert!(q.balance.is_none());
    }

    #[test]
    fn parse_plan_label_from_result() {
        let body =
            r#"{"Result":{"PlanName":"ark pro","QuotaUsage":[{"Level":"session","Percent":10}]}}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("Ark Pro"));
    }

    #[test]
    fn parse_lowercase_level() {
        let body = r#"{"Result":{"QuotaUsage":[{"level":"monthly","percent":85.0}]}}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert!((q.windows[0].used_pct - 85.0).abs() < 1e-6);
        assert_eq!(q.windows[0].label, "月");
        assert_eq!(q.status, QuotaStatus::Danger);
    }

    #[test]
    fn parse_empty_errors() {
        assert!(matches!(parse(r#"{"Result":{}}"#), Err(VendorError::Empty)));
    }

    #[test]
    fn ark_key_probe_and_missing_secret_errors() {
        // Ark key flow: Bearer on CodingPlan (fails) → chat probe (succeeds).
        struct ArkMock;
        impl Http for ArkMock {
            fn post(
                &self,
                url: &str,
                headers: &[(String, String)],
                _: &str,
            ) -> Result<String, VendorError> {
                // Bearer on GetCodingPlanUsage — simulate failure.
                if url == URL {
                    assert!(headers
                        .iter()
                        .any(|(k, v)| k == "Authorization" && v.starts_with("Bearer ark-")));
                    return Err(VendorError::Empty);
                }
                unreachable!("unexpected post URL: {url}")
            }
            fn post_with_response_headers(
                &self,
                _: &str,
                _: &[(String, String)],
                _: &str,
            ) -> Result<(String, Vec<(String, String)>), VendorError> {
                let resp_headers = vec![
                    ("x-ratelimit-remaining-requests".into(), "140".into()),
                    ("x-ratelimit-limit-requests".into(), "200".into()),
                    ("x-ratelimit-reset-requests".into(), "3600".into()),
                ];
                Ok(("{}".into(), resp_headers))
            }
        }
        let q = fetch_with(&ArkMock, r#"{"key":"ark-xxx","secret":""}"#).unwrap();
        assert_eq!(q.vendor, "volcengine");
        assert_eq!(q.plan_label.as_deref(), Some("Ark API"));
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "Ark");
        // remaining 140/200 = 70% left → used 30%
        assert!((q.windows[0].used_pct - 30.0).abs() < 1e-6);
        assert!(q.windows[0].resets_at.is_some());

        // Ark key with Bearer CodingPlan succeeding → 3 windows (preferred path).
        struct ArkMockCodingPlan;
        impl Http for ArkMockCodingPlan {
            fn post(
                &self,
                url: &str,
                headers: &[(String, String)],
                _: &str,
            ) -> Result<String, VendorError> {
                if url == URL {
                    assert!(headers
                        .iter()
                        .any(|(k, v)| k == "Authorization" && v.starts_with("Bearer ark-")));
                    return Ok(r#"{"Result":{"QuotaUsage":[
                        {"Level":"5-hour","Percent":40.0,"ResetTimestamp":1893456000}
                    ]}}"#
                        .into());
                }
                unreachable!()
            }
            fn post_with_response_headers(
                &self,
                _: &str,
                _: &[(String, String)],
                _: &str,
            ) -> Result<(String, Vec<(String, String)>), VendorError> {
                unreachable!()
            }
        }
        let q2 = fetch_with(&ArkMockCodingPlan, r#"{"key":"ark-xxx","secret":""}"#).unwrap();
        assert_eq!(q2.plan_label.as_deref(), Some("Coding Plan (Ark)"));
        assert_eq!(q2.windows.len(), 1);
        assert_eq!(q2.windows[0].label, "5h");
        assert!((q2.windows[0].used_pct - 40.0).abs() < 1e-6);

        // AK without secret → error, no HTTP call.
        struct FailMock;
        impl Http for FailMock {
            fn post(
                &self,
                _: &str,
                _: &[(String, String)],
                _: &str,
            ) -> Result<String, VendorError> {
                unreachable!("no HTTP call when AK+SK secret is missing")
            }
        }
        assert!(fetch_with(&FailMock, r#"{"key":"AKLTID","secret":""}"#).is_err());
    }

    #[test]
    fn signed_headers_shape() {
        let h = signed_headers(
            "AKLTID",
            "secret",
            "20260101T000000Z",
            "20260101",
            "cn-beijing",
        );
        let auth = h.iter().find(|(k, _)| k == "Authorization").unwrap();
        assert!(auth
            .1
            .starts_with("HMAC-SHA256 Credential=AKLTID/20260101/cn-beijing/ark/request"));
        assert!(auth
            .1
            .contains("SignedHeaders=content-type;host;x-content-sha256;x-date"));
        assert!(h.iter().any(|(k, _)| k == "X-Date"));
    }

    #[test]
    fn empty_body_hash_is_known_constant() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn extract_csrf_from_cookie() {
        let c = "foo=1; csrfToken=6fb21460dc555bc1d100a2ef7235279e; bar=2";
        assert_eq!(
            extract_csrf(c).as_deref(),
            Some("6fb21460dc555bc1d100a2ef7235279e")
        );
        assert!(extract_csrf("no=csrf").is_none());
    }

    #[test]
    fn parse_subscribe_info_running() {
        let body = r#"{"Result":{"InfoList":[{"ResourceType":"CodingPlan","BizInfo":"lite","Status":"Running","EndTime":"2026-08-09T15:59:59Z"}]}}"#;
        let info = parse_subscribe_info(body).unwrap().expect("should be Some");
        assert_eq!(info.end_time.as_deref(), Some("2026-08-09T15:59:59Z"));
        assert_eq!(info.plan_label.as_deref(), Some("Coding Plan Lite"));
    }

    #[test]
    fn parse_subscribe_info_picks_running() {
        // Mix of Expired + Running → pick Running, with its tier + end time.
        let body = r#"{"Result":{"InfoList":[
            {"BizInfo":"lite","Status":"Expired","EndTime":"2025-01-01T00:00:00Z"},
            {"BizInfo":"pro","Status":"Running","EndTime":"2026-08-09T15:59:59Z"}
        ]}}"#;
        let info = parse_subscribe_info(body).unwrap().expect("should be Some");
        assert_eq!(info.plan_label.as_deref(), Some("Coding Plan Pro"));
        assert_eq!(info.end_time.as_deref(), Some("2026-08-09T15:59:59Z"));
    }

    #[test]
    fn parse_subscribe_info_empty() {
        assert!(parse_subscribe_info(r#"{"Result":{"InfoList":[]}}"#)
            .unwrap()
            .is_none());
    }

    #[test]
    fn format_plan_from_tiers() {
        let mk = |biz: Option<&str>| SubscribeTradeInfo {
            Status: None,
            EndTime: None,
            BizInfo: biz.map(String::from),
            ProductName: None,
            PlanName: None,
        };
        assert_eq!(
            format_plan_from_info(&mk(Some("lite"))).as_deref(),
            Some("Coding Plan Lite")
        );
        assert_eq!(
            format_plan_from_info(&mk(Some("pro"))).as_deref(),
            Some("Coding Plan Pro")
        );
        assert!(format_plan_from_info(&mk(None)).is_none());
        assert!(format_plan_from_info(&mk(Some(""))).is_none());
    }

    #[test]
    fn apply_end_sets_expires_at_only() {
        // The 5h window keeps its own rolling ResetTimestamp; the plan end
        // goes to `expires_at`, NOT onto the windows.
        let mut q = Quota {
            vendor: "volcengine".into(),
            status: QuotaStatus::Ok,
            windows: vec![
                QuotaWindow {
                    label: "5h".into(),
                    used_pct: 10.0,
                    resets_at: Some("2026-07-25T03:00:00Z".into()),
                    ..Default::default()
                },
                QuotaWindow {
                    label: "周".into(),
                    used_pct: 20.0,
                    resets_at: None,
                    ..Default::default()
                },
            ],
            balance: None,
            plan_label: Some("Coding Plan".into()),
            refreshed_at: None,
            error: None,
            cookie_error: None,
            expires_at: None,
        };
        apply_subscription_end(&mut q, "2026-08-09T15:59:59Z");
        // Plan end → expires_at.
        assert_eq!(q.expires_at.as_deref(), Some("2026-08-09T15:59:59Z"));
        // Per-window resets_at untouched (5h keeps its own, 周 stays None).
        assert_eq!(
            q.windows[0].resets_at.as_deref(),
            Some("2026-07-25T03:00:00Z")
        );
        assert!(q.windows[1].resets_at.is_none());
        assert_eq!(q.windows[0].used_pct, 10.0); // untouched
    }
}
