//! Kimi Code usage adapter.
//!
//! Two auth modes (token-monitor src/shared/kimiLimits.js):
//! 1. **API Key** → `GET /coding/v1/usages` with Bearer → 5h + weekly windows.
//! 2. **Web Token** (JWT/kimi-auth cookie) → two POSTs to web API →
//!    5h + 7d + monthly windows + subscription balance.
//!
//! When both are provided, results are merged (web takes priority for richer data).
//!
//! Credential format: `{"key":"sk-...", "web_token":"jwt..."}`

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::{epoch_to_iso, parse_iso, Quota, QuotaStatus, QuotaWindow};
use super::VendorError;

// Code API
const CODE_URL: &str = "https://api.kimi.com/coding/v1/usages";
// Web API
const WEB_USAGES_URL: &str =
    "https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages";
const WEB_MEMBERSHIP_URL: &str =
    "https://www.kimi.com/apiv2/kimi.gateway.membership.v2.MembershipService/GetSubscriptionStats";

const SESSION_MAX_MINUTES: f64 = 6.0 * 60.0;

// ── Credential (backward-compatible: plain string = API key) ────────────────

struct Credential {
    key: String,
    web_token: String,
}

/// Normalize a Kimi web token from various paste formats (token-monitor
/// `normalizeKimiWebToken`). Accepts a raw JWT value, a `kimi-auth=VALUE`
/// cookie form, or a `authorization: bearer VALUE` / `bearer VALUE` prefixed
/// form. Rejects curl commands or multi-cookie strings (those contain `;`).
fn normalize_web_token(raw: &str) -> String {
    let mut s = raw.trim().trim_matches('"').to_string();
    if s.is_empty() {
        return String::new();
    }
    // Strip `authorization:` then `bearer ` prefix (case-insensitive, sequential).
    loop {
        let lower = s.to_ascii_lowercase();
        let stripped = lower
            .strip_prefix("authorization:")
            .map(|_| "authorization:".len())
            .or_else(|| lower.strip_prefix("bearer ").map(|_| "bearer ".len()))
            .or_else(|| {
                lower
                    .strip_prefix("authorization ")
                    .map(|_| "authorization ".len())
            })
            .or_else(|| lower.strip_prefix("bearer:").map(|_| "bearer:".len()));
        match stripped {
            Some(len) => s = s[len..].trim().to_string(),
            None => break,
        }
    }
    // Extract `kimi-auth=VALUE` if present.
    let lower = s.to_ascii_lowercase();
    if let Some(pos) = lower.find("kimi-auth=") {
        let after = &s[pos + "kimi-auth=".len()..];
        let end = after
            .find(|c: char| c == ';' || c.is_whitespace() || c == '\'' || c == '"')
            .unwrap_or(after.len());
        return after[..end].trim().to_string();
    }
    // Reject curl commands or multi-cookie strings.
    let lower2 = s.to_ascii_lowercase();
    if lower2.starts_with("cookie:") || lower2.starts_with("curl ") || s.contains(';') {
        return String::new();
    }
    s
}

fn parse_credential(raw: &str) -> Result<Credential, VendorError> {
    // Try JSON object first.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(obj) = v.as_object() {
            // New format: {"cookie": "..."} (from Account.svelte)
            if let Some(cookie_val) = obj.get("cookie").and_then(|v| v.as_str()) {
                let token = normalize_web_token(cookie_val);
                if !token.is_empty() {
                    return Ok(Credential {
                        key: String::new(),
                        web_token: token,
                    });
                }
            }
            // Legacy format: {"key": "...", "web_token": "..."}
            return Ok(Credential {
                key: obj
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                web_token: normalize_web_token(
                    obj.get("web_token").and_then(|v| v.as_str()).unwrap_or(""),
                ),
            });
        }
    }
    // Fallback: plain string → web token (JWT from kimi-auth cookie).
    let trimmed = raw.trim().trim_matches('"');
    if !trimmed.is_empty() {
        Ok(Credential {
            key: String::new(),
            web_token: normalize_web_token(trimmed),
        })
    } else {
        Err(VendorError::Parse("empty credential".into()))
    }
}

// ── Common types ────────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct Detail {
    #[serde(default, alias = "usedValue", alias = "used_value")]
    used: Option<f64>,
    #[serde(default, alias = "limitValue", alias = "total", alias = "quota")]
    limit: Option<f64>,
    #[serde(default, alias = "remainingValue", alias = "remaining_value")]
    remaining: Option<f64>,
    #[serde(
        default,
        alias = "percentage",
        alias = "usedPercent",
        alias = "used_percent"
    )]
    percent: Option<f64>,
    #[serde(
        default,
        alias = "resetTime",
        alias = "reset_time",
        alias = "resetAt",
        alias = "reset_at"
    )]
    reset_time: Option<Value>,
    #[serde(default, alias = "label", alias = "title")]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LimitEntry {
    #[serde(default, alias = "usage", alias = "quota")]
    detail: Option<Detail>,
    #[serde(default, alias = "period", alias = "rateLimit", alias = "rate_limit")]
    window: Option<WindowDesc>,
    #[serde(flatten)]
    inline: Detail,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct WindowDesc {
    #[serde(
        default,
        alias = "windowDuration",
        alias = "window_duration",
        alias = "size",
        alias = "value",
        alias = "length"
    )]
    duration: Option<f64>,
    #[serde(
        default,
        alias = "time_unit",
        alias = "unit",
        alias = "windowUnit",
        alias = "window_unit"
    )]
    time_unit: Option<String>,
}

// ── Code API response ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct CodeResp {
    #[serde(default)]
    usage: Option<Detail>,
    #[serde(default)]
    limits: Vec<LimitEntry>,
}

// ── Web API responses ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct WebUsageResp {
    #[serde(default)]
    data: Option<WebUsageData>,
}
#[derive(Debug, Default, Deserialize)]
struct WebUsageData {
    #[serde(default)]
    usages: Vec<WebUsageEntry>,
}
#[derive(Debug, Deserialize)]
struct WebUsageEntry {
    #[serde(default)]
    scope: String,
    #[serde(default)]
    detail: Option<Detail>,
    #[serde(default)]
    limits: Vec<LimitEntry>,
}

/// Kimi Membership Stats response.
///
/// The API returns fields at top level (no `data` wrapper), but we also support
/// the `data`-wrapped format for compatibility.
#[derive(Debug, Deserialize)]
struct MembershipResp {
    #[serde(default)]
    data: Option<MembershipData>,
    // Top-level fallbacks when there's no `data` wrapper.
    #[serde(
        default,
        alias = "ratelimitCode5h",
        alias = "ratelimit_code_5h",
        alias = "ratelimit5h"
    )]
    ratelimit_5h: Option<RateLimitWindow>,
    #[serde(
        default,
        alias = "ratelimitCode7d",
        alias = "ratelimit_code_7d",
        alias = "ratelimit7d"
    )]
    ratelimit_7d: Option<RateLimitWindow>,
    #[serde(default, alias = "subscriptionBalance", alias = "subscription_balance")]
    subscription_balance: Option<SubscriptionBalance>,
}
#[derive(Debug, Default, Deserialize)]
struct MembershipData {
    #[serde(
        default,
        alias = "ratelimitCode5h",
        alias = "ratelimit_code_5h",
        alias = "ratelimit5h"
    )]
    ratelimit_5h: Option<RateLimitWindow>,
    #[serde(
        default,
        alias = "ratelimitCode7d",
        alias = "ratelimit_code_7d",
        alias = "ratelimit7d"
    )]
    ratelimit_7d: Option<RateLimitWindow>,
    #[serde(default, alias = "subscriptionBalance", alias = "subscription_balance")]
    subscription_balance: Option<SubscriptionBalance>,
}
#[derive(Debug, Default, Deserialize)]
struct RateLimitWindow {
    #[serde(default)]
    ratio: Option<f64>,
    #[serde(default, alias = "usedRatio", alias = "used_ratio")]
    used_ratio: Option<f64>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(
        default,
        alias = "resetTime",
        alias = "reset_time",
        alias = "resetAt",
        alias = "reset_at"
    )]
    reset_time: Option<Value>,
}
#[derive(Debug, Default, Deserialize)]
struct SubscriptionBalance {
    #[serde(default)]
    feature: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default, alias = "amountUsedRatio", alias = "amount_used_ratio")]
    amount_used_ratio: Option<f64>,
    #[serde(default, alias = "expireTime", alias = "expire_time")]
    expire_time: Option<Value>,
}

// ── HTTP trait ──────────────────────────────────────────────────────────────

pub trait Http {
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError>;
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<String, VendorError>;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn window_minutes(duration: Option<f64>, unit: &Option<String>) -> Option<f64> {
    let amount = duration?;
    if amount <= 0.0 {
        return None;
    }
    let u = unit.as_deref().unwrap_or("").to_ascii_uppercase();
    if u.contains("MIN") {
        Some(amount)
    } else if u.contains("HOUR") {
        Some(amount * 60.0)
    } else if u.contains("DAY") {
        Some(amount * 24.0 * 60.0)
    } else if u.contains("WEEK") {
        Some(amount * 7.0 * 24.0 * 60.0)
    } else if u.contains("MONTH") {
        Some(amount * 30.0 * 24.0 * 60.0)
    } else {
        None
    }
}

fn used_pct(d: &Detail) -> Option<f64> {
    if let (Some(used), Some(limit)) = (d.used, d.limit) {
        if limit > 0.0 {
            return Some((used / limit * 100.0).clamp(0.0, 100.0));
        }
    }
    if let (Some(limit), Some(remaining)) = (d.limit, d.remaining) {
        if limit > 0.0 {
            return Some(((limit - remaining) / limit * 100.0).clamp(0.0, 100.0));
        }
    }
    d.percent.map(|p| p.clamp(0.0, 100.0))
}

fn reset_iso(v: &Option<Value>) -> Option<String> {
    match v {
        Some(Value::Number(n)) => n.as_f64().and_then(epoch_to_iso),
        Some(Value::String(s)) => parse_iso(s),
        _ => None,
    }
}

fn is_session_name(name: &Option<String>) -> bool {
    let raw = name.as_deref().unwrap_or("").to_lowercase();
    raw.contains("hour") || raw.contains("小时") || raw.contains("時間") || raw.contains("시간")
}

// ── Build web token headers (JWT-derived) ──────────────────────────────────

fn web_headers(token: &str) -> Vec<(String, String)> {
    let mut h = vec![
        ("Authorization".into(), format!("Bearer {token}")),
        ("Cookie".into(), format!("kimi-auth={token}")),
        ("Content-Type".into(), "application/json".into()),
        ("Accept".into(), "application/json".into()),
        ("Origin".into(), "https://www.kimi.com".into()),
        ("Referer".into(), "https://www.kimi.com/code/console".into()),
        ("connect-protocol-version".into(), "1".into()),
        ("x-language".into(), "en-US".into()),
        ("x-msh-platform".into(), "web".into()),
    ];
    // JWT session headers from token payload (best-effort).
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() == 3 {
        if let Ok(padded) = decode_base64url(parts[1]) {
            if let Ok(payload) = serde_json::from_str::<Value>(&padded) {
                if let Some(device_id) = payload.get("device_id").and_then(|v| v.as_str()) {
                    h.push(("x-msh-device-id".into(), device_id.to_string()));
                }
                if let Some(ssid) = payload.get("ssid").and_then(|v| v.as_str()) {
                    h.push(("x-msh-session-id".into(), ssid.to_string()));
                }
                if let Some(sub) = payload.get("sub").and_then(|v| v.as_str()) {
                    h.push(("x-traffic-id".into(), sub.to_string()));
                }
            }
        }
    }
    h
}

fn decode_base64url(input: &str) -> Result<String, VendorError> {
    // Add padding.
    let padded = match input.len() % 4 {
        2 => format!("{input}=="),
        3 => format!("{input}="),
        _ => input.to_string(),
    };
    // Replace URL-safe chars.
    let standard = padded.replace('-', "+").replace('_', "/");
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&standard)
        .map_err(|e| VendorError::Parse(format!("base64: {e}")))?;
    String::from_utf8(bytes).map_err(|e| VendorError::Parse(format!("utf8: {e}")))
}

// ── Code API parse ──────────────────────────────────────────────────────────

pub fn parse(body: &str) -> Result<Quota, VendorError> {
    let resp: CodeResp =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    let mut windows: Vec<QuotaWindow> = Vec::new();
    let mut have_session = false;
    let mut have_weekly = false;

    for entry in &resp.limits {
        let detail = entry.detail.as_ref().unwrap_or(&entry.inline);
        let pct = match used_pct(detail) {
            Some(p) => p,
            None => continue,
        };
        let minutes = entry
            .window
            .as_ref()
            .and_then(|w| window_minutes(w.duration, &w.time_unit));
        let is_session =
            matches!(minutes, Some(m) if m <= SESSION_MAX_MINUTES) || minutes.is_none();
        let (label, session) = if is_session {
            ("5h", true)
        } else {
            ("周", false)
        };
        if session {
            have_session = true;
        } else {
            have_weekly = true;
        }
        windows.push(QuotaWindow {
            label: label.into(),
            used_pct: pct,
            resets_at: reset_iso(&detail.reset_time),
            ..Default::default()
        });
    }

    if let Some(usage) = &resp.usage {
        if let Some(pct) = used_pct(usage) {
            let session = is_session_name(&usage.name);
            let already = (session && have_session) || (!session && have_weekly);
            if !already {
                windows.push(QuotaWindow {
                    label: if session { "5h" } else { "周" }.into(),
                    used_pct: pct,
                    resets_at: reset_iso(&usage.reset_time),
                    ..Default::default()
                });
            }
        }
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
        site: None,
        vendor: "kimi".into(),
        plan_label: None,
        status,
        windows,
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
    })
}

// ── Web API parse ───────────────────────────────────────────────────────────

/// Convert a 0..1 usage ratio to a 0..100 percentage, clamped.
///
/// Kimi's `ratio` / `usedRatio` / `amountUsedRatio` fields are fractions where
/// 1.0 = fully consumed. Over-quota usage legitimately reports values > 1.0
/// (e.g. 1.02 = exceeded by 2%); these must be treated as ratios and clamped
/// to 100%, NOT reinterpreted as already-percent values (which would render a
/// spent plan as nearly full). Always multiply by 100 then clamp.
fn ratio_to_pct(ratio: f64) -> f64 {
    (ratio * 100.0).clamp(0.0, 100.0)
}

/// Parse the membership stats response into windows (5h, 7d, monthly).
fn parse_membership(body: &str) -> Vec<QuotaWindow> {
    let resp: MembershipResp = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    // Use `data` wrapper if present, otherwise use top-level fields.
    let rl_5h = resp
        .data
        .as_ref()
        .and_then(|d| d.ratelimit_5h.as_ref())
        .or(resp.ratelimit_5h.as_ref());
    let rl_7d = resp
        .data
        .as_ref()
        .and_then(|d| d.ratelimit_7d.as_ref())
        .or(resp.ratelimit_7d.as_ref());
    let sb = resp
        .data
        .as_ref()
        .and_then(|d| d.subscription_balance.as_ref())
        .or(resp.subscription_balance.as_ref());

    let mut windows = Vec::new();

    // 5-hour rate limit
    if let Some(rl) = rl_5h {
        if rl.enabled != Some(false) {
            let ratio = rl.ratio.or(rl.used_ratio).unwrap_or(0.0);
            let used_pct = ratio_to_pct(ratio);
            windows.push(QuotaWindow {
                label: "5h".into(),
                used_pct,
                resets_at: reset_iso(&rl.reset_time),
                ..Default::default()
            });
        }
    }

    // 7-day rate limit
    if let Some(rl) = rl_7d {
        if rl.enabled != Some(false) {
            let ratio = rl.ratio.or(rl.used_ratio).unwrap_or(0.0);
            let used_pct = ratio_to_pct(ratio);
            windows.push(QuotaWindow {
                label: "周".into(),
                used_pct,
                resets_at: reset_iso(&rl.reset_time),
                ..Default::default()
            });
        }
    }

    // Monthly billing (subscription balance)
    if let Some(sb) = sb {
        let feature = sb.feature.as_deref().unwrap_or("");
        let type_ = sb.r#type.as_deref().unwrap_or("");
        let compatible = (feature.is_empty() || feature == "FEATURE_OMNI")
            && (type_.is_empty() || type_ == "SUBSCRIPTION");
        if compatible {
            if let Some(ratio) = sb.amount_used_ratio {
                let used_pct = ratio_to_pct(ratio);
                windows.push(QuotaWindow {
                    label: "月".into(),
                    used_pct,
                    resets_at: reset_iso(&sb.expire_time),
                    ..Default::default()
                });
            }
        }
    }

    windows
}

/// Parse web usage response into windows (5h, weekly from code usage API).
///
/// The API may return:
/// 1. Standard format: `{"data": {"usages": [{"scope": "FEATURE_CODING", ...}]}}`
/// 2. Degraded format: `{"totalQuota": {"limit": "100", "remaining": "100"}}`
fn parse_web_usage(body: &str) -> Vec<QuotaWindow> {
    let resp: WebUsageResp = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let data = match resp.data {
        Some(d) => d,
        None => return vec![],
    };
    let coding = data
        .usages
        .into_iter()
        .find(|e| e.scope == "FEATURE_CODING");
    let entry = match coding {
        Some(e) => e,
        None => return vec![],
    };

    // Reuse Code API parse logic by building a synthetic payload.
    let synthetic = serde_json::json!({
        "usage": entry.detail,
        "limits": entry.limits,
    });
    match serde_json::from_value::<CodeResp>(synthetic) {
        Ok(resp) => {
            let mut windows = Vec::new();
            let mut have_session = false;
            let mut have_weekly = false;
            for lim in &resp.limits {
                let d = lim.detail.as_ref().unwrap_or(&lim.inline);
                if let Some(pct) = used_pct(d) {
                    let minutes = lim
                        .window
                        .as_ref()
                        .and_then(|w| window_minutes(w.duration, &w.time_unit));
                    let sess =
                        matches!(minutes, Some(m) if m <= SESSION_MAX_MINUTES) || minutes.is_none();
                    windows.push(QuotaWindow {
                        label: if sess { "5h".into() } else { "周".into() },
                        used_pct: pct,
                        resets_at: reset_iso(&d.reset_time),
                        ..Default::default()
                    });
                    if sess {
                        have_session = true;
                    } else {
                        have_weekly = true;
                    }
                }
            }
            if let Some(usage) = &resp.usage {
                if let Some(pct) = used_pct(usage) {
                    let sess = is_session_name(&usage.name);
                    let already = (sess && have_session) || (!sess && have_weekly);
                    if !already {
                        windows.push(QuotaWindow {
                            label: if sess { "5h" } else { "周" }.into(),
                            used_pct: pct,
                            resets_at: reset_iso(&usage.reset_time),
                            ..Default::default()
                        });
                    }
                }
            }
            windows
        }
        Err(_) => vec![],
    }
}

/// Merge windows from different sources, deduplicating by label.
fn merge_windows(groups: Vec<Vec<QuotaWindow>>) -> Vec<QuotaWindow> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for group in groups {
        for w in group {
            if seen.insert(w.label.clone()) {
                out.push(w);
            }
        }
    }
    // Sort by label priority: 5h → 周 → 月
    out.sort_by_key(|w| match w.label.as_str() {
        "5h" => 0,
        "周" => 1,
        "月" => 2,
        _ => 3,
    });
    out
}

// ── Fetch ───────────────────────────────────────────────────────────────────

/// Fetch with both API key and web token (credential is JSON blob or plain string).
pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let cred = parse_credential(credential)?;
    let has_key = !cred.key.is_empty();
    let has_web = !cred.web_token.is_empty();

    if !has_key && !has_web {
        return Err(VendorError::Parse(
            "no credential: provide key or web_token".into(),
        ));
    }

    let mut windows: Vec<Vec<QuotaWindow>> = Vec::new();
    // Track whether the web token itself was rejected (401/403) so we can
    // surface an auth error instead of a silent "no data" when the cookie
    // expires (Kimi's primary credential is the web token).
    let mut web_auth_failed = false;

    // Web token path (richer data: 5h + 7d + monthly)
    if has_web {
        super::validate_header_safe(&cred.web_token)?;
        let headers = web_headers(&cred.web_token);

        // Membership stats
        match http.post(WEB_MEMBERSHIP_URL, &headers, "{}") {
            Ok(body) => {
                let mw = parse_membership(&body);
                if !mw.is_empty() {
                    windows.push(mw);
                }
            }
            Err(e) => {
                if super::is_auth_error(&e) {
                    web_auth_failed = true;
                }
                tracing::warn!(error = %e, "kimi membership POST failed");
            }
        }

        // Web usage
        let usage_body = serde_json::json!({"scope": ["FEATURE_CODING"]}).to_string();
        match http.post(WEB_USAGES_URL, &headers, &usage_body) {
            Ok(body) => {
                let uw = parse_web_usage(&body);
                if !uw.is_empty() {
                    windows.push(uw);
                }
            }
            Err(e) => {
                if super::is_auth_error(&e) {
                    web_auth_failed = true;
                }
                tracing::warn!(error = %e, "kimi usage POST failed");
            }
        }
    }

    // API Key path (fallback for session + weekly if web missed them)
    if has_key {
        super::validate_header_safe(&cred.key)?;
        if let Ok(body) = http.get(CODE_URL, &cred.key) {
            if let Ok(q) = parse(&body) {
                let key_windows: Vec<QuotaWindow> = q
                    .windows
                    .into_iter()
                    .filter(|w| w.label != "MCP 月")
                    .collect();
                if !key_windows.is_empty() {
                    windows.push(key_windows);
                }
            }
        }
    }

    let merged = merge_windows(windows);
    if merged.is_empty() {
        // If the web token was rejected, surface it as an auth error so the
        // scheduler marks the card with `cookie_error` (inline update hint)
        // instead of a generic empty.
        if web_auth_failed {
            return Err(VendorError::Network("status code 401".into()));
        }
        return Err(VendorError::Empty);
    }
    let status = QuotaStatus::worst_of(
        merged
            .iter()
            .map(|w| QuotaStatus::from_used_pct(w.used_pct)),
    );
    Ok(Quota {
        site: None,
        vendor: "kimi".into(),
        plan_label: None,
        status,
        windows: merged,
        balance: None,
        refreshed_at: None,
        error: None,
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
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError> {
        let resp = crate::utils::http::direct_agent()
            .get(url)
            .set("Authorization", &format!("Bearer {bearer}"))
            .set("Accept", "application/json")
            .call()
            .map_err(|e| VendorError::Network(e.to_string()))?;
        resp.into_string()
            .map_err(|e| VendorError::Network(e.to_string()))
    }

    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<String, VendorError> {
        let mut req = crate::utils::http::direct_agent().post(url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        let resp = req
            .send_string(body)
            .map_err(|e| VendorError::Network(e.to_string()))?;
        resp.into_string()
            .map_err(|e| VendorError::Network(e.to_string()))
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_session_and_weekly() {
        let body = r#"{
            "usage":{"used":400,"limit":1000,"reset_at":"2030-01-01T00:00:00Z"},
            "limits":[{"detail":{"used":600,"limit":1000,"resetTime":1893456000},
                       "window":{"duration":300,"time_unit":"TIME_UNIT_MINUTE"}}]
        }"#;
        let q = parse(body).unwrap();
        assert_eq!(q.windows.len(), 2);
        let five = q.windows.iter().find(|w| w.label == "5h").unwrap();
        assert!((five.used_pct - 60.0).abs() < 1e-6);
        assert!(five.resets_at.is_some());
        let weekly = q.windows.iter().find(|w| w.label == "周").unwrap();
        assert!((weekly.used_pct - 40.0).abs() < 1e-6);
    }

    #[test]
    fn parse_weekly_only_from_usage() {
        let body = r#"{"usage":{"usedPercent":72.0}}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "周");
        assert!((q.windows[0].used_pct - 72.0).abs() < 1e-6);
    }

    #[test]
    fn parse_empty_errors() {
        assert!(matches!(parse(r#"{}"#), Err(VendorError::Empty)));
    }

    #[test]
    fn parse_membership_basic() {
        let body = r#"{"data":{
            "ratelimitCode5h":{"ratio":0.3,"resetTime":"2030-06-01T00:00:00Z"},
            "ratelimitCode7d":{"ratio":0.5,"resetTime":"2030-06-07T00:00:00Z"},
            "subscriptionBalance":{"feature":"FEATURE_OMNI","type":"SUBSCRIPTION","amountUsedRatio":0.6,"expireTime":"2030-07-01T00:00:00Z"}
        }}"#;
        let w = parse_membership(body);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].label, "5h");
        assert!((w[0].used_pct - 30.0).abs() < 1e-6);
        assert_eq!(w[1].label, "周");
        assert!((w[1].used_pct - 50.0).abs() < 1e-6);
        assert_eq!(w[2].label, "月");
        assert!((w[2].used_pct - 60.0).abs() < 1e-6);
    }

    /// Over-quota ratios (>1.0) must clamp to 100%, not be misread as an
    /// already-percentage value (regression for the heuristic-normalization bug).
    #[test]
    fn parse_membership_over_quota_clamps_to_full() {
        // 5h ratio 1.02 (exceeded by 2%), weekly 1.5 (exceeded by 50%),
        // monthly 0.0 (none used).
        let body = r#"{"data":{
            "ratelimitCode5h":{"ratio":1.02},
            "ratelimitCode7d":{"ratio":1.5},
            "subscriptionBalance":{"feature":"FEATURE_OMNI","type":"SUBSCRIPTION","amountUsedRatio":0.0}
        }}"#;
        let w = parse_membership(body);
        assert_eq!(w.len(), 3);
        let five = w.iter().find(|x| x.label == "5h").unwrap();
        assert!(
            (five.used_pct - 100.0).abs() < 1e-6,
            "over-quota 5h should clamp to 100%, got {}",
            five.used_pct
        );
        let week = w.iter().find(|x| x.label == "周").unwrap();
        assert!(
            (week.used_pct - 100.0).abs() < 1e-6,
            "over-quota weekly should clamp to 100%, got {}",
            week.used_pct
        );
        let month = w.iter().find(|x| x.label == "月").unwrap();
        assert!((month.used_pct - 0.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_web_token_handles_formats() {
        // Raw JWT.
        assert_eq!(normalize_web_token("eyJabc.def.ghi"), "eyJabc.def.ghi");
        // kimi-auth cookie form.
        assert_eq!(
            normalize_web_token("kimi-auth=eyJabc.def.ghi"),
            "eyJabc.def.ghi"
        );
        assert_eq!(
            normalize_web_token("foo=1; kimi-auth=eyJabc.def.ghi; bar=2"),
            "eyJabc.def.ghi"
        );
        // Authorization / Bearer prefixes.
        assert_eq!(
            normalize_web_token("authorization: bearer eyJabc"),
            "eyJabc"
        );
        assert_eq!(normalize_web_token("Bearer eyJabc"), "eyJabc");
        // Reject curl / multi-cookie.
        assert_eq!(normalize_web_token("curl https://kimi.com"), "");
        assert_eq!(normalize_web_token("cookie: a=1; b=2"), "");
        assert_eq!(normalize_web_token("a=1; b=2"), "");
    }

    #[test]
    fn parse_credential_accepts_plain_string() {
        let c = parse_credential("sk-test123").unwrap();
        assert!(c.key.is_empty());
        assert_eq!(c.web_token, "sk-test123");
    }

    #[test]
    fn parse_credential_accepts_json_object() {
        // Legacy format
        let c = parse_credential(r#"{"key":"sk-test","web_token":"jwt-test"}"#).unwrap();
        assert_eq!(c.key, "sk-test");
        assert_eq!(c.web_token, "jwt-test");
        // New format (cookie field)
        let c2 = parse_credential(r#"{"cookie":"eyJabc.def.ghi"}"#).unwrap();
        assert!(c2.key.is_empty());
        assert_eq!(c2.web_token, "eyJabc.def.ghi");
    }

    #[test]
    fn parse_real_kimi_token() {
        let token = "eyJhbGciOiJIUzUxMiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJ1c2VyLWNlbnRlciIsImV4cCI6MTc4Njc2MzM3MywiaWF0IjoxNzg0MTcxMzczLCJqdGkiOiJkOWM0bXJjZGVpamxvcXZrYWp0ZyIsInR5cCI6ImFjY2VzcyIsImFwcF9pZCI6ImtpbWkiLCJzdWIiOiJjc3VpcmlibXZxOGtnYW04Z2d2MCIsInNwYWNlX2lkIjoiY3N1aXJpYm12cThrZ2FtOGdndWciLCJhYnN0cmFjdF91c2VyX2lkIjoiY3N1aXJpYm12cThrZ2FtOGdndTAiLCJzc2lkIjoiMTczMTA5MDg5ODAyNjI2MjY2MiIsImRldmljZV9pZCI6Ijc2NjI5NTc1MjU2OTU3ODk4MzQiLCJyZWdpb24iOiJjbiIsIm1lbWJlcnNoaXAiOnsibGV2ZWwiOjEwfX0.Np2_2UpVxx2Qhu3pCraxCON23RXwOueQrxODbCZQT7cyZAUiC9giEuvFeqeN0HFzB02ejefFytAegAwcUcWjgw";

        let json = format!(r#"{{"cookie": "{}"}}"#, token);
        let c = parse_credential(&json).unwrap();
        assert!(c.key.is_empty());
        assert!(!c.web_token.is_empty());
        assert_eq!(c.web_token.len(), token.len());
        println!(
            "✓ Kimi token parsed successfully, length: {}",
            c.web_token.len()
        );
    }

    #[test]
    fn parse_credential_rejects_empty() {
        assert!(parse_credential("").is_err());
    }

    #[test]
    fn fetch_with_uses_both_credentials() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, url: &str, _: &str) -> Result<String, VendorError> {
                assert_eq!(url, CODE_URL);
                Ok(r#"{"limits":[{"detail":{"used":600,"limit":1000},"window":{"duration":300,"time_unit":"TIME_UNIT_MINUTE"}}]}"#.into())
            }
            fn post(
                &self,
                url: &str,
                _: &[(String, String)],
                _: &str,
            ) -> Result<String, VendorError> {
                if url.contains("Membership") {
                    return Ok(r#"{"data":{"ratelimitCode5h":{"ratio":0.2},"ratelimitCode7d":{"ratio":0.4},"subscriptionBalance":{"amountUsedRatio":0.5}}}"#.into());
                }
                Ok(r#"{"data":{"usages":[]}}"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"key":"sk-test","web_token":"jwt-test"}"#).unwrap();
        // Should have 3 windows from membership + likely 5h from code API
        assert_eq!(q.windows.len(), 3);
        // 5h from membership (20%) merged with 5h from code (60%) → take first (membership)
        let five = q.windows.iter().find(|w| w.label == "5h").unwrap();
        assert!((five.used_pct - 20.0).abs() < 1e-6);
    }
}
