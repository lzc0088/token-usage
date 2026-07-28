//! MiMo (小米) Token Plan adapter.
//!
//! Cookie-based auth only (token-monitor src/shared/mimoLimits.js).
//! Required cookies: `api-platform_serviceToken`, `userId`.
//!
//! GET /api/v1/balance          → balance (amount, currency)
//! GET /api/v1/tokenPlan/usage  → monthly token usage
//! GET /api/v1/tokenPlan/detail → plan info (active/expired, planCode, resetsAt)

use serde::Deserialize;

use super::types::{Quota, QuotaBalance, QuotaStatus, QuotaWindow};
use super::VendorError;

const BASE_URL: &str = "https://platform.xiaomimimo.com/api/v1";

pub trait Http {
    fn get_with_cookie(&self, url: &str, cookie: &str) -> Result<String, VendorError>;
}

// ── API response types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    #[serde(default)]
    data: Option<T>,
}

#[derive(Debug, Default, Deserialize)]
struct BalanceData {
    #[serde(default)]
    balance: Option<serde_json::Value>,
    #[serde(default, alias = "cashBalance", alias = "cash_balance")]
    cash_balance: Option<serde_json::Value>,
    #[serde(default, alias = "giftBalance", alias = "gift_balance")]
    gift_balance: Option<serde_json::Value>,
    #[serde(default)]
    currency: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PlanUsageData {
    #[serde(default, alias = "monthUsage", alias = "month_usage")]
    month_usage: Option<MonthlyUsage>,
}
#[derive(Debug, Default, Deserialize)]
struct MonthlyUsage {
    #[serde(default)]
    items: Vec<PlanUsageItem>,
}
#[derive(Debug, Default, Deserialize)]
struct PlanUsageItem {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    used: Option<f64>,
    #[serde(default)]
    limit: Option<f64>,
    #[allow(dead_code)]
    #[serde(default)]
    percent: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct PlanDetailData {
    #[serde(default, alias = "planCode", alias = "plan_code")]
    plan_code: Option<String>,
    #[allow(dead_code)]
    #[serde(default, alias = "planName", alias = "plan_name")]
    plan_name: Option<String>,
    #[serde(default, alias = "currentPeriodEnd", alias = "current_period_end")]
    current_period_end: Option<String>,
    #[serde(default)]
    expired: Option<bool>,
    #[serde(default)]
    active: Option<bool>,
    #[serde(default, alias = "planStatus", alias = "plan_status")]
    plan_status: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn number_from_str(s: &str) -> Option<f64> {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    cleaned.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Parse a `serde_json::Value` that may be a number or string into f64.
fn value_to_f64(v: Option<serde_json::Value>) -> Option<f64> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => number_from_str(&s),
        _ => None,
    }
}

/// Normalize cookie: strip `Cookie:` prefix, keep MiMo allowlist, check required.
fn normalize_cookie(raw: &str) -> String {
    let raw = raw.trim();
    // Strip "Cookie:" prefix that users may paste from DevTools.
    let raw = if let Some(rest) = raw
        .strip_prefix("Cookie:")
        .or_else(|| raw.strip_prefix("cookie:"))
    {
        rest.trim()
    } else {
        raw
    };
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut has_service_token = false;
    let mut has_user_id = false;
    for part in raw.split(';') {
        let eq = part.find('=').unwrap_or(0);
        if eq == 0 {
            continue;
        }
        let name = part[..eq].trim();
        let value = part[eq + 1..].trim().trim_matches('"');
        let keep = matches!(
            name,
            "api-platform_serviceToken" | "userId" | "api-platform_ph" | "api-platform_slh"
        );
        if keep && !value.is_empty() {
            if name == "api-platform_serviceToken" {
                has_service_token = true;
            }
            if name == "userId" {
                has_user_id = true;
            }
            pairs.push((name.to_string(), value.to_string()));
        }
    }
    if !has_service_token || !has_user_id {
        return String::new();
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
        .into_iter()
        .map(|(n, v)| format!("{n}={v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

// ── Parse ───────────────────────────────────────────────────────────────────

pub fn parse_balance(body: &str) -> Result<QuotaBalance, VendorError> {
    let env: ApiEnvelope<BalanceData> =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    let data = env
        .data
        .ok_or(VendorError::Parse("no balance data".into()))?;
    let amount = value_to_f64(data.balance)
        .or_else(|| {
            let cash = value_to_f64(data.cash_balance).unwrap_or(0.0);
            let gift = value_to_f64(data.gift_balance).unwrap_or(0.0);
            if cash > 0.0 || gift > 0.0 {
                Some(cash + gift)
            } else {
                None
            }
        })
        .ok_or(VendorError::Parse("no balance amount".into()))?;
    let currency = data.currency.unwrap_or_else(|| "USD".into()).to_uppercase();
    Ok(QuotaBalance {
        amount,
        currency,
        today_consumption: None,
        month_consumption: None,
    })
}

/// Parse the plan detail. Returns `(window, plan_code, expires_at)` where
/// `expires_at` is the subscription's `currentPeriodEnd` (plan end date).
pub fn parse_plan(
    body: &str,
) -> Result<Option<(QuotaWindow, String, Option<String>)>, VendorError> {
    let env: ApiEnvelope<PlanDetailData> =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    let data = match env.data {
        Some(d) => d,
        None => return Ok(None),
    };

    let plan_code = data.plan_code.as_deref().unwrap_or("").to_string();
    let plan_status = data.plan_status.unwrap_or_default().to_lowercase();

    // Check if this is a known no-plan code (token-monitor: MIMO_NO_PLAN_CODES).
    let is_no_plan = matches!(
        plan_code.as_str(),
        "default" | "none" | "no_plan" | "not_subscribed" | "unsubscribed"
    );
    if is_no_plan {
        return Ok(None);
    }

    // Check if expired.
    let is_expired =
        data.expired.unwrap_or(false) || plan_status == "expired" || plan_status == "ended";

    // Check if active (token-monitor: explicitActive OR hasFuturePeriod).
    let explicit_active =
        data.active.unwrap_or(false) || plan_status == "active" || plan_status == "subscribed";
    let has_real_plan_identity = !plan_code.is_empty();
    let has_future_period = data.current_period_end.as_deref().is_some_and(|s| {
        let s = s.trim();
        if s.len() < 10 {
            return false;
        }
        let dt = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"));
        if let Ok(dt) = dt {
            // Treat the naive timestamp as China Standard Time (UTC+8).
            let cst = match chrono::FixedOffset::east_opt(8 * 3600) {
                Some(tz) => tz,
                None => return false,
            };
            match dt.and_local_timezone(cst).single() {
                Some(future) => future.with_timezone(&chrono::Utc) > chrono::Utc::now(),
                None => false,
            }
        } else {
            false
        }
    });

    // Activate if: explicitActive OR (has real plan identity AND future period)
    let is_active = explicit_active || (has_real_plan_identity && has_future_period);
    if !is_active || is_expired {
        return Ok(None);
    }

    let resets_at = data.current_period_end.as_deref().and_then(|s| {
        let s = s.trim();
        if s.len() < 10 {
            return None;
        }
        let dt = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
            .ok()?;
        // MiMo returns time in China Standard Time (UTC+8), not UTC. Attach
        // the +08:00 offset before converting to a normalized RFC3339 UTC
        // timestamp so the frontend renders the correct calendar date.
        let cst = chrono::FixedOffset::east_opt(8 * 3600)?;
        let with_tz = dt.and_local_timezone(cst).single()?;
        Some(with_tz.with_timezone(&chrono::Utc).to_rfc3339())
    });

    Ok(Some((
        QuotaWindow {
            label: if plan_code.is_empty() {
                "Token Plan".into()
            } else {
                plan_code.clone()
            },
            used_pct: 0.0,   // Will be filled from usage
            resets_at: None, // Plan expiry goes to Quota.expires_at, not here
            ..Default::default()
        },
        plan_code,
        resets_at, // plan expiry (currentPeriodEnd) → surfaced as Quota.expires_at
    )))
}

pub fn parse_usage(body: &str) -> Result<Option<(f64, f64)>, VendorError> {
    let env: ApiEnvelope<PlanUsageData> =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    let data = match env.data {
        Some(d) => d,
        None => return Ok(None),
    };
    let usage = data.month_usage.unwrap_or_default();
    let total = usage
        .items
        .into_iter()
        .find(|i| i.name.as_deref().unwrap_or("").to_lowercase() == "month_total_token");
    match total {
        Some(t) => {
            let used = t.used.unwrap_or(0.0);
            let limit = t.limit.unwrap_or(0.0);
            if limit > 0.0 {
                Ok(Some((used, limit)))
            } else {
                Ok(Some((used, 0.0)))
            }
        }
        None => Ok(None),
    }
}

// ── Fetch ───────────────────────────────────────────────────────────────────

/// Fetch MiMo quota. `credential` is the raw cookie string.
pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    // credential may be JSON `{"cookie":"..."}` or plain cookie string.
    let raw_cookie = serde_json::from_str::<serde_json::Value>(credential)
        .ok()
        .and_then(|v| {
            v.get("cookie")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| credential.to_string());
    let cookie = normalize_cookie(&raw_cookie);
    if cookie.is_empty() {
        return Err(VendorError::Parse(
            "缺少必需 Cookie：api-platform_serviceToken 和 userId".into(),
        ));
    }

    let fetch_api = |path: &str| -> Result<String, VendorError> {
        let url = format!("{BASE_URL}{path}");
        http.get_with_cookie(&url, &cookie)
    };

    // Balance (optional for tp- keys that may not have balance access)
    let balance = fetch_api("/balance")
        .ok()
        .and_then(|b| parse_balance(&b).ok());

    // Plan detail (optional)
    let mut plan_label: Option<String> = None;
    let mut windows: Vec<QuotaWindow> = Vec::new();
    let mut plan_expires_at: Option<String> = None;
    if let Ok(detail_body) = fetch_api("/tokenPlan/detail") {
        if let Ok(Some((window, code, expiry))) = parse_plan(&detail_body) {
            plan_label = Some(if code.is_empty() {
                "MiMo Token Plan".into()
            } else {
                // Capitalize first letter: "lite" -> "Lite", "standard" -> "Standard"
                let mut chars = code.chars();
                let capitalized = match chars.next() {
                    Some(first) => {
                        let upper: String = first.to_uppercase().collect();
                        upper + chars.as_str()
                    }
                    None => String::new(),
                };
                format!("MiMo Token Plan {capitalized}")
            });
            windows.push(window);
            plan_expires_at = expiry;
        }
    }

    // Usage (optional)
    if let Ok(usage_body) = fetch_api("/tokenPlan/usage") {
        if let Ok(Some((used, limit))) = parse_usage(&usage_body) {
            if let Some(w) = windows.first_mut() {
                w.used_pct = if limit > 0.0 {
                    (used / limit * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                };
            }
        }
    }

    if balance.is_none() && windows.is_empty() {
        return Err(VendorError::Empty);
    }
    let status = QuotaStatus::from_used_pct(windows.first().map(|w| w.used_pct).unwrap_or(0.0));
    Ok(Quota {
        vendor: "mimo".into(),
        plan_label,
        status,
        windows,
        balance,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: plan_expires_at,
    })
}

pub async fn fetch(cookie_str: &str) -> Result<Quota, VendorError> {
    let cookie = cookie_str.to_string();
    tokio::task::spawn_blocking(move || fetch_with(UreqHttp::instance(), &cookie))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

struct UreqHttp {
    agent: ureq::Agent,
}
impl UreqHttp {
    fn instance() -> &'static Self {
        use std::sync::OnceLock;
        static INSTANCE: OnceLock<UreqHttp> = OnceLock::new();
        INSTANCE.get_or_init(|| Self {
            agent: ureq::AgentBuilder::new()
                .redirects(0) // match token-monitor redirect: 'manual'
                .build(),
        })
    }
    fn request(&self, url: &str, cookie: &str) -> ureq::Request {
        self.agent
            .get(url)
            .set("Cookie", cookie)
            .set("Accept", "application/json, text/plain, */*")
            .set("Accept-Language", "en-US,en;q=0.9")
            .set("Origin", "https://platform.xiaomimimo.com")
            .set(
                "Referer",
                "https://platform.xiaomimimo.com/#/console/balance",
            )
            .set(
                "User-Agent",
                "Mozilla/5.0 AppleWebKit/537.36 Chrome/143 Safari/537.36",
            )
    }
}
impl Http for UreqHttp {
    fn get_with_cookie(&self, url: &str, cookie: &str) -> Result<String, VendorError> {
        match self.request(url, cookie).call() {
            Ok(resp) => {
                let body = resp
                    .into_string()
                    .map_err(|e| VendorError::Network(e.to_string()))?;
                // Check body.code field (token-monitor: code !== 0 → rejected).
                check_body_code(&body)?;
                Ok(body)
            }
            Err(ureq::Error::Status(code, resp)) => {
                // token-monitor treats 401/403 AND 3xx redirects as session-expired.
                let body_str = resp.into_string().unwrap_or_default();
                if code == 401 || code == 403 || (300..400).contains(&code) {
                    return Err(VendorError::Network("status code 401".into()));
                }
                // 429 = rate limited.
                if code == 429 {
                    return Err(VendorError::Network("status code 429".into()));
                }
                // Also check body.code field.
                check_body_code(&body_str)?;
                Err(VendorError::Network(format!("status code {code}")))
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

/// Validate `body.code` (token-monitor: 0 = success, 401/403 = unauthorized).
fn check_body_code(body: &str) -> Result<(), VendorError> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Ok(()), // not JSON, let the parser handle it
    };
    let code = v.get("code").and_then(|c| c.as_i64());
    match code {
        Some(401) | Some(403) => Err(VendorError::Network("status code 401".into())),
        Some(c) if c != 0 => {
            let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
            Err(VendorError::Network(format!(
                "api rejected: code {c} {msg}"
            )))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_cookie_with_quoted_values() {
        let raw = r#"userId=9596700; api-platform_serviceToken="yVHEDQlNiF..."; api-platform_slh="WKMj/TcCwcD="; api-platform_ph="zA36XbYDYeHn""#;
        let n = normalize_cookie(raw);
        assert!(n.contains("api-platform_serviceToken=yVHEDQlNiF..."));
        assert!(n.contains("userId=9596700"));
        assert!(!n.contains("\""));
    }

    #[test]
    fn normalize_cookie_filters_and_sorts() {
        let raw = "unrelated=drop; userId=123; api-platform_serviceToken=secret; api-platform_ph=optional";
        let n = normalize_cookie(raw);
        assert!(n.contains("api-platform_ph=optional"));
        assert!(n.contains("api-platform_serviceToken=secret"));
        assert!(n.contains("userId=123"));
        assert!(!n.contains("unrelated"));
    }

    #[test]
    fn parse_balance_envelope() {
        let body = r#"{"data":{"balance":"25.51","currency":"usd","cashBalance":"20","giftBalance":"5.51"}}"#;
        let b = parse_balance(body).unwrap();
        assert!((b.amount - 25.51).abs() < 0.01);
        assert_eq!(b.currency, "USD");
    }

    #[test]
    fn parse_usage_item() {
        let body = r#"{"data":{"monthUsage":{"items":[{"name":"month_total_token","used":10,"limit":100,"percent":0.1}]}}}"#;
        let u = parse_usage(body).unwrap();
        assert!(u.is_some());
        let (used, limit) = u.unwrap();
        assert!((used - 10.0).abs() < 0.1);
        assert!((limit - 100.0).abs() < 0.1);
    }

    #[test]
    fn fetch_with_cookie() {
        struct Mock;
        impl Http for Mock {
            fn get_with_cookie(&self, url: &str, _: &str) -> Result<String, VendorError> {
                if url.contains("/balance") {
                    Ok(r#"{"data":{"balance":"100","currency":"CNY"}}"#.into())
                } else if url.contains("/tokenPlan/detail") {
                    Ok(r#"{"data":{"planCode":"Standard","currentPeriodEnd":"2099-01-01T00:00:00Z","active":true}}"#.into())
                } else if url.contains("/tokenPlan/usage") {
                    Ok(r#"{"data":{"monthUsage":{"items":[{"name":"month_total_token","used":30,"limit":200,"percent":0.15}]}}}"#.into())
                } else {
                    Err(VendorError::Empty)
                }
            }
        }
        let q = fetch_with(&Mock, "api-platform_serviceToken=secret; userId=123").unwrap();
        assert_eq!(q.vendor, "mimo");
        assert!((q.balance.as_ref().unwrap().amount - 100.0).abs() < 0.01);
        assert_eq!(q.windows.len(), 1);
        assert!((q.windows[0].used_pct - 15.0).abs() < 0.1);
        assert_eq!(q.plan_label.as_deref(), Some("MiMo Token Plan Standard"));
    }

    #[test]
    fn parse_real_mimo_cookie() {
        let cookie = r#"userId=9596700; api-platform_serviceToken="yVHEDQlNiFT1WavLtPFnewAQELgC/os0pGsdriY0wMWX02Icm5nr7GvS3CPBL8lKl9N2QrN6EtEeFBXsvXzYZ92Plj9b5KWASoWuzjSMVEIyAPtfIKYq32QMSa2W2Dp91uEDFDuhMlwis2j7lj0M6+lT32kPBc4Hw3iXGbj1oF3DnN03pb32aTMB+csEYh/Ji929UVUAn7ALqgXmA05KjKYXPA9G1IjIzKgvMjcH4z/raWqBtWu4wF7LLgYkiiB3viLrCSwS9iIzVEBHw25DM0rQIXin2nlumYnn6Jzl73uHSzrgx1lWhDq2M1Xp2otmzVaAWX+K6UQRPOqVd2mqRGce9pfaJBHEGfJJQ9q76I="; api-platform_slh="WKMj/TcCwcDQRdQmHC7n7KoBkAU="; api-platform_ph="zA36XbYDYeHn71mnQVifpA==""#;

        // Use serde_json::json! for proper escaping (cookie values contain quotes)
        let json = serde_json::json!({"cookie": cookie}).to_string();

        // Test that JSON parsing works
        let raw_cookie = serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .and_then(|v| {
                v.get("cookie")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        assert!(
            !raw_cookie.is_empty(),
            "raw_cookie should not be empty, json was: {}",
            &json[..json.len().min(100)]
        );

        // Test normalize_cookie
        let normalized = normalize_cookie(&raw_cookie);
        assert!(
            !normalized.is_empty(),
            "normalized cookie should not be empty"
        );
        assert!(normalized.contains("api-platform_serviceToken"));
        assert!(normalized.contains("userId=9596700"));
        println!(
            "✓ MiMo cookie parsed successfully, normalized length: {}",
            normalized.len()
        );
    }

    #[test]
    fn parse_plan_skips_default_plan() {
        let body = r#"{"data":{"planCode":"default","currentPeriodEnd":"2099-01-01T00:00:00Z","active":true}}"#;
        let result = parse_plan(body).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_plan_accepts_active_without_status() {
        let body = r#"{"data":{"planCode":"Standard","currentPeriodEnd":"2099-01-01 00:00:00","active":true}}"#;
        let result = parse_plan(body).unwrap();
        assert!(result.is_some());
        let (window, code, expiry) = result.unwrap();
        assert_eq!(code, "Standard");
        // Plan window's resets_at is None; expiry moved to the 3rd tuple slot.
        assert!(window.resets_at.is_none());
        assert!(expiry.is_some());
    }

    /// Real MiMo /tokenPlan/detail payload — no explicit `active`/`planStatus`,
    /// but `planCode` + future `currentPeriodEnd` + `expired: false` mean the
    /// plan should be recognized as active.
    #[test]
    fn parse_plan_real_lite_payload() {
        let body = r#"{"code":0,"message":"","data":{"planCode":"lite","planName":"Lite","currentPeriodEnd":"2099-08-24 23:59:59","expired":false,"enableAutoRenew":false,"autoRenewDiscount":null,"hasAutoRenewSubscribed":true,"clawEnabled":false,"clawPeriodEnd":null,"clawPurchased":false}}"#;
        let result = parse_plan(body).unwrap();
        assert!(result.is_some(), "should recognize lite plan as active");
        let (window, code, expiry) = result.unwrap();
        assert_eq!(code, "lite");
        assert!(window.resets_at.is_none(), "resets_at moved to expiry");
        assert!(expiry.is_some(), "expiry (currentPeriodEnd) should be set");
    }
}
