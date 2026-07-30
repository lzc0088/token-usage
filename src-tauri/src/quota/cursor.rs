//! Cursor IDE subscription adapter.
//!
//! Cookie-based: the user pastes the `WorkosCursorSessionToken` cookie value
//! (just the value, not `key=value`). Three GETs against cursor.com:
//!   GET /api/usage-summary  → individualUsage + teamUsage (cents-based)
//!   GET /api/auth/me        → `sub` (user id, feeds the request-usage call)
//!   GET /api/usage?user=sub → gpt-4 request counts (optional)
//!
//! All amounts are in cents; divided by 100 for USD. We surface the primary
//! plan window (Pro plan usage) as the quota. token-monitor also tracks
//! on-demand credits and team pools — we fold the most relevant one into the
//! plan window when the explicit plan block is empty.
//!
//! Faithfully ported from token-monitor src/shared/cursorProbe.js.

use serde::Deserialize;

use super::types::{Quota, QuotaStatus, QuotaWindow};
use super::VendorError;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

const USAGE_SUMMARY_URL: &str = "https://cursor.com/api/usage-summary";
#[allow(dead_code)]
const AUTH_ME_URL: &str = "https://cursor.com/api/auth/me";
#[allow(dead_code)]
const REQUEST_USAGE_URL: &str = "https://cursor.com/api/usage";

/// HTTP client. Injected for unit tests.
pub trait Http {
    fn get(&self, url: &str, cookie: &str) -> Result<String, VendorError>;
}

fn number(v: Option<&serde_json::Value>) -> Option<f64> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

#[allow(dead_code)]
fn cents_to_usd(v: Option<&serde_json::Value>) -> Option<f64> {
    number(v).map(|c| c.round() / 100.0)
}

#[allow(dead_code)]
fn clamp_pct(n: Option<f64>) -> Option<f64> {
    n.map(|v| v.clamp(0.0, 100.0))
}

fn pct_from_used_limit(used: Option<f64>, limit: Option<f64>) -> Option<f64> {
    let (u, l) = (used?, limit?);
    if l <= 0.0 {
        return None;
    }
    Some(((u / l) * 100.0).clamp(0.0, 100.0))
}

/// `/api/auth/me` response — only `sub` is needed.
#[derive(Debug, Default, Deserialize)]
struct UserInfo {
    #[serde(default)]
    sub: Option<String>,
}

/// `/api/usage-summary` payload (only the fields we consume).
#[derive(Debug, Default, Deserialize)]
#[allow(non_snake_case)]
struct UsageSummary {
    #[serde(default, alias = "individualUsage")]
    individual_usage: Option<IndividualUsage>,
    #[serde(default, alias = "teamUsage")]
    team_usage: Option<TeamUsage>,
    // Kept for deserialization; not displayed in the quota UI.
    #[serde(default, alias = "billingCycleEnd")]
    _billing_cycle_end: Option<String>,
    #[serde(default, alias = "membershipType")]
    membership_type: Option<String>,
    #[serde(default, alias = "isUnlimited")]
    is_unlimited: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct IndividualUsage {
    #[serde(default)]
    plan: Option<Bucket>,
    #[serde(default)]
    overall: Option<Bucket>,
}
#[derive(Debug, Default, Deserialize)]
struct TeamUsage {
    #[serde(default)]
    pooled: Option<Bucket>,
}
#[derive(Debug, Default, Deserialize)]
struct Bucket {
    #[serde(default)]
    used: Option<serde_json::Value>,
    #[serde(default)]
    limit: Option<serde_json::Value>,
    #[serde(default, alias = "totalPercentUsed")]
    total_percent_used: Option<serde_json::Value>,
}

impl Bucket {
    fn used(&self) -> Option<f64> {
        number(self.used.as_ref())
    }
    fn limit(&self) -> Option<f64> {
        number(self.limit.as_ref())
    }
    fn total_pct(&self) -> Option<f64> {
        number(self.total_percent_used.as_ref())
    }
}

/// Map the raw `membershipType` (e.g. "free", "pro") to a friendly plan label.
fn membership_label(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    let mapped = match lower.as_str() {
        "free" => Some("Free Plan"),
        "pro" => Some("Pro Plan"),
        "pro_trial" | "protrial" | "pro_free" | "trial" => Some("Pro Trial"),
        "team" | "teams" => Some("Teams Plan"),
        "enterprise" => Some("Enterprise"),
        "unlimited" => Some("Unlimited"),
        _ => None,
    };
    if let Some(s) = mapped {
        return s.into();
    }
    // Fallback: Title-case the raw token + " Plan".
    let mut chars = lower.chars();
    match chars.next() {
        Some(c) => {
            let head = c.to_uppercase().collect::<String>();
            format!("{head}{} Plan", chars.as_str())
        }
        None => "Plan".into(),
    }
}

/// Resolve the plan used% from the usage-summary payload.
///
/// Priority (token-monitor parseUsageSummary):
/// 1. plan.totalPercentUsed (authoritative, returned by the API)
/// 2. plan.used / plan.limit
/// 3. overall used/limit
/// 4. team pooled used/limit
///
/// Falls back to 0 when nothing is reported.
fn plan_used_pct(summary: &UsageSummary) -> f64 {
    let plan = summary
        .individual_usage
        .as_ref()
        .and_then(|i| i.plan.as_ref());
    if let Some(plan) = plan {
        if let Some(p) = plan.total_pct() {
            return p.clamp(0.0, 100.0);
        }
        if let (Some(u), Some(l)) = (plan.used(), plan.limit()) {
            if l > 0.0 {
                return ((u / l) * 100.0).clamp(0.0, 100.0);
            }
        }
    }
    let overall = summary
        .individual_usage
        .as_ref()
        .and_then(|i| i.overall.as_ref());
    if let Some(ov) = overall {
        if let Some(p) = pct_from_used_limit(ov.used(), ov.limit()) {
            return p;
        }
    }
    let pooled = summary.team_usage.as_ref().and_then(|t| t.pooled.as_ref());
    if let Some(p) = pooled {
        if let Some(pct) = pct_from_used_limit(p.used(), p.limit()) {
            return pct;
        }
    }
    0.0
}

/// Extract the plan bucket's raw used / limit (for the credits caption).
fn plan_used_limit(summary: &UsageSummary) -> (Option<f64>, Option<f64>) {
    let plan = summary
        .individual_usage
        .as_ref()
        .and_then(|i| i.plan.as_ref());
    match plan {
        Some(p) => (p.used(), p.limit()),
        None => (None, None),
    }
}

/// Parse `/api/auth/me` to extract the user `sub`.
#[allow(dead_code)]
pub fn parse_sub(body: &str) -> Option<String> {
    let info: UserInfo = serde_json::from_str(body).ok()?;
    info.sub.filter(|s| !s.is_empty())
}

/// Fetch via `http`. `credential` is the raw cookie value or `{"cookie": ...}`.
pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let token = serde_json::from_str::<serde_json::Value>(credential)
        .ok()
        .and_then(|v| {
            v.get("cookie")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| credential.to_string());
    let token = token.trim();
    if token.is_empty() {
        return Err(VendorError::Parse("缺少 Cookie".into()));
    }
    // The cookie value is the token; cursor expects `WorkosCursorSessionToken=<v>`.
    // If the user pasted a full cookie string, pass it through unchanged.
    let cookie = if token.contains('=') {
        token.to_string()
    } else {
        format!("WorkosCursorSessionToken={token}")
    };

    let summary_body = http.get(USAGE_SUMMARY_URL, &cookie)?;
    let summary: UsageSummary =
        serde_json::from_str(&summary_body).map_err(|e| VendorError::Parse(e.to_string()))?;

    let is_unlimited = summary.is_unlimited.unwrap_or(false);
    let used_pct = if is_unlimited {
        0.0
    } else {
        plan_used_pct(&summary)
    };
    let (plan_used, plan_limit) = plan_used_limit(&summary);

    // Plan label from membershipType (e.g. "free" → "Free Plan"). Unlimited
    // accounts override the label so the user sees their tier at a glance.
    let plan_label = if is_unlimited {
        Some("Unlimited".to_string())
    } else {
        summary.membership_type.as_deref().map(membership_label)
    };

    let window = QuotaWindow {
        label: "Plan".into(),
        used_pct,
        // billingCycleEnd is not displayed; Cursor's billing cycle is implicit.
        resets_at: None,
        used_value: plan_used,
        total_value: plan_limit,
        ..Default::default()
    };
    Ok(Quota {
        vendor: "cursor".into(),
        plan_label,
        status: QuotaStatus::from_used_pct(used_pct),
        windows: vec![window],
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
    })
}

/// Default fetch (real network). The auth/me + usage?user=sub calls are
/// optional enrichment; the primary window comes from usage-summary.
pub async fn fetch(credential: &str) -> Result<Quota, VendorError> {
    let cred = credential.to_string();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &cred))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

struct UreqHttp;
impl Http for UreqHttp {
    fn get(&self, url: &str, cookie: &str) -> Result<String, VendorError> {
        let resp = ureq::get(url)
            .set("Cookie", cookie)
            .set("Accept", "*/*")
            .set("Accept-Language", "en-US,en;q=0.9")
            .set("Referer", "https://www.cursor.com/settings")
            .set("User-Agent", USER_AGENT)
            .call();
        match resp {
            Ok(r) => r
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string())),
            Err(ureq::Error::Status(code, _r)) => {
                if code == 401 || code == 403 {
                    Err(VendorError::Network("status code 401".into()))
                } else if code == 429 {
                    Err(VendorError::Network("status code 429".into()))
                } else {
                    Err(VendorError::Network(format!("status code {code}")))
                }
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_pct_prefers_total_percent_used() {
        // Real Cursor response: plan carries totalPercentUsed; prefer it.
        let body = r#"{"individualUsage":{"plan":{"used":0,"limit":0,"totalPercentUsed":42}}}"#;
        let s: UsageSummary = serde_json::from_str(body).unwrap();
        assert!((plan_used_pct(&s) - 42.0).abs() < 1e-6);
    }

    #[test]
    fn plan_pct_from_plan_bucket() {
        let body = r#"{"individualUsage":{"plan":{"used":3000,"limit":10000}}}"#;
        let s: UsageSummary = serde_json::from_str(body).unwrap();
        assert!((plan_used_pct(&s) - 30.0).abs() < 1e-6);
    }

    #[test]
    fn plan_pct_falls_back_to_overall() {
        let body = r#"{"individualUsage":{"overall":{"used":5000,"limit":10000}}}"#;
        let s: UsageSummary = serde_json::from_str(body).unwrap();
        assert!((plan_used_pct(&s) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn plan_pct_falls_back_to_team_pooled() {
        let body = r#"{"teamUsage":{"pooled":{"used":2000,"limit":8000}}}"#;
        let s: UsageSummary = serde_json::from_str(body).unwrap();
        assert!((plan_used_pct(&s) - 25.0).abs() < 1e-6);
    }

    #[test]
    fn plan_pct_zero_when_empty() {
        let body = r#"{}"#;
        let s: UsageSummary = serde_json::from_str(body).unwrap();
        assert!((plan_used_pct(&s) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn parse_sub_extracts_user_id() {
        let body = r#"{"sub":"user-123","email":"a@b.com"}"#;
        assert_eq!(parse_sub(body).as_deref(), Some("user-123"));
    }

    #[test]
    fn fetch_with_wraps_bare_token_into_cookie() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _url: &str, cookie: &str) -> Result<String, VendorError> {
                assert_eq!(cookie, "WorkosCursorSessionToken=abc123");
                Ok(r#"{"individualUsage":{"plan":{"used":8000,"limit":10000}}}"#.into())
            }
        }
        let q = fetch_with(&Mock, "abc123").unwrap();
        assert_eq!(q.vendor, "cursor");
        assert!((q.windows[0].used_pct - 80.0).abs() < 1e-6);
        assert_eq!(q.status, QuotaStatus::Danger);
    }

    #[test]
    fn fetch_with_passes_through_full_cookie_string() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _url: &str, cookie: &str) -> Result<String, VendorError> {
                assert!(cookie.contains("WorkosCursorSessionToken=abc"));
                assert!(cookie.contains("other=1"));
                Ok(r#"{"individualUsage":{"plan":{"used":1000,"limit":10000}}}"#.into())
            }
        }
        let q = fetch_with(&Mock, "WorkosCursorSessionToken=abc; other=1").unwrap();
        assert!((q.windows[0].used_pct - 10.0).abs() < 1e-6);
    }

    #[test]
    fn fetch_with_accepts_json_cookie_blob() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                Ok(r#"{"individualUsage":{"plan":{"used":5000,"limit":10000}}}"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"cookie":"raw-token"}"#).unwrap();
        assert!((q.windows[0].used_pct - 50.0).abs() < 1e-6);
    }

    #[test]
    fn membership_label_maps_known_tiers() {
        assert_eq!(membership_label("free"), "Free Plan");
        assert_eq!(membership_label("PRO"), "Pro Plan");
        assert_eq!(membership_label("pro_trial"), "Pro Trial");
        assert_eq!(membership_label("teams"), "Teams Plan");
        assert_eq!(membership_label("enterprise"), "Enterprise");
        // Unknown → Title-case + " Plan"
        assert_eq!(membership_label("business"), "Business Plan");
    }

    #[test]
    fn fetch_with_uses_membership_type_and_billing_cycle() {
        // Real-shape response (free plan, zero usage).
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                Ok(r#"{"billingCycleStart":"2026-07-08T11:29:58.520Z","billingCycleEnd":"2026-08-08T11:29:58.520Z","membershipType":"free","isUnlimited":false,"individualUsage":{"plan":{"enabled":true,"used":0,"limit":0,"remaining":0,"totalPercentUsed":0}}}"#.into())
            }
        }
        let q = fetch_with(&Mock, "tok").unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("Free Plan"));
        // Cursor intentionally hides reset time (billing cycle is implicit).
        assert!(q.windows[0].resets_at.is_none());
        assert!((q.windows[0].used_pct - 0.0).abs() < 1e-6);
    }

    #[test]
    fn fetch_with_unlimited_forces_zero_pct() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                Ok(r#"{"membershipType":"pro","isUnlimited":true,"billingCycleEnd":"2026-08-08T11:29:58.520Z","individualUsage":{"plan":{"used":9999,"limit":1000,"totalPercentUsed":100}}}"#.into())
            }
        }
        let q = fetch_with(&Mock, "tok").unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("Unlimited"));
        assert!((q.windows[0].used_pct - 0.0).abs() < 1e-6);
    }

    #[test]
    fn fetch_with_requires_token() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                unreachable!()
            }
        }
        let err = fetch_with(&Mock, "  ").unwrap_err();
        assert!(matches!(err, VendorError::Parse(_)));
    }
}
