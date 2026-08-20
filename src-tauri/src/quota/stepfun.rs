//! StepFun (阶跃星辰) adapter — cookie-based console Connect-RPC API.
//!
//! Auth is via the `platform.stepfun.com` session Cookie (must contain
//! `Oasis-Token`; `Oasis-Webid` is extracted and also sent as a header).
//!
//! Three endpoints are called (same POST + oasis-* headers, empty JSON body):
//!
//! 1. QueryAccountBalance → `balance` (CNY), `credit`
//! 2. GetStepPlanStatus  → subscription name, expiry, plan name
//! 3. QueryStepPlanRateLimit → usage % for Step Plan / 5h / weekly windows
//!
//! Results are merged into a single [`Quota`]: the plan name becomes
//! `plan_label` (e.g. "Step Plus"), the account balance (CNY) goes into
//! `balance`, and active windows carry usage % + reset times.

use serde::Deserialize;

use super::types::{epoch_to_iso, Quota, QuotaBalance, QuotaStatus, QuotaWindow};
use super::VendorError;

const BALANCE_URL: &str =
    "https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/QueryAccountBalance";
const PLAN_URL: &str =
    "https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/GetStepPlanStatus";
const RATE_LIMIT_URL: &str =
    "https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/QueryStepPlanRateLimit";
const OASIS_APPID: &str = "10300";

pub trait Http {
    /// POST a Connect-RPC call with cookie + oasis-* headers (empty JSON body).
    fn connect_rpc(
        &self,
        url: &str,
        cookie: &str,
        oasis_webid: &str,
    ) -> Result<String, VendorError>;
}

// ── API response types ──────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct AccountBalance {
    #[serde(default)]
    balance: Option<String>,
    #[serde(default)]
    credit: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StepPlanStatus {
    #[allow(dead_code)]
    status: Option<i32>,
    #[serde(default)]
    subscription: Option<SubscriptionInfo>,
}

#[derive(Debug, Default, Deserialize)]
struct SubscriptionInfo {
    name: Option<String>,
    #[allow(dead_code)]
    status: Option<i32>, // 1 = active
    #[serde(default)]
    #[allow(dead_code)]
    activated_at: Option<String>,
    #[serde(default)]
    expired_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StepPlanRateLimit {
    #[serde(default)]
    five_hour_usage_left_rate: Option<f64>,
    #[serde(default)]
    five_hour_usage_reset_time: Option<String>,
    #[serde(default)]
    weekly_usage_left_rate: Option<f64>,
    #[serde(default)]
    weekly_usage_reset_time: Option<String>,
    #[serde(default)]
    plan_credit_rate_limit: Option<PlanCreditRateLimit>,
}

#[derive(Debug, Default, Deserialize)]
struct PlanCreditRateLimit {
    #[serde(default)]
    subscription_credit_left_rate: Option<f64>,
    #[serde(default)]
    subscription_credit_reset_time: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    credit_buckets: Option<Vec<CreditBucket>>,
}

#[derive(Debug, Default, Deserialize)]
struct CreditBucket {
    #[allow(dead_code)]
    r#type: Option<i32>,
    #[allow(dead_code)]
    credit_total: Option<String>,
    #[allow(dead_code)]
    credit_residual: Option<String>,
    #[allow(dead_code)]
    expire_at: Option<String>,
    #[allow(dead_code)]
    next_reset_at: Option<String>,
}

#[derive(Debug, Default)]
struct RateLimitWindows {
    /// Step Plan subscription usage % (derived from `1 - left_rate`).
    plan_used_pct: Option<f64>,
    /// Credit reset timestamp (monthly rollover, not plan expiry).
    plan_resets_at: Option<String>,
    /// 5-hour rolling window (some plans).
    five_h_used_pct: Option<f64>,
    five_h_resets_at: Option<String>,
    /// Weekly window (some plans).
    weekly_used_pct: Option<f64>,
    weekly_resets_at: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn str_to_f64(s: &Option<String>) -> f64 {
    s.as_deref()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

fn cookie_value(cookie: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(&prefix) {
            return Some(rest.to_string());
        }
    }
    None
}

fn normalize_cookie(raw: &str) -> String {
    let raw = raw.trim();
    let raw = if let Some(rest) = raw
        .strip_prefix("Cookie:")
        .or_else(|| raw.strip_prefix("cookie:"))
    {
        rest.trim()
    } else {
        raw
    };
    raw.to_string()
}

/// `left_rate` is the remaining FRACTION (0..1). Convert to used % (0..100).
fn left_rate_to_used_pct(left_rate: f64) -> f64 {
    ((1.0 - left_rate.clamp(0.0, 1.0)) * 100.0).clamp(0.0, 100.0)
}

/// Parse an epoch string that may be "0" (→ None, window not applicable).
fn epoch_opt(s: &Option<String>) -> Option<f64> {
    s.as_deref()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0)
}

// ── Parse ───────────────────────────────────────────────────────────────────

fn parse_balance(body: &str) -> Result<(f64, f64), VendorError> {
    let resp: AccountBalance = serde_json::from_str(body)
        .map_err(|e| VendorError::Parse(format!("stepfun balance: {e}")))?;
    Ok((str_to_f64(&resp.balance), str_to_f64(&resp.credit)))
}

/// Parse the Step Plan subscription info. Returns `(plan_name, expired_at_iso)`.
fn parse_plan(body: &str) -> Result<Option<(String, Option<String>)>, VendorError> {
    let resp: StepPlanStatus =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(format!("stepfun plan: {e}")))?;
    match resp.subscription {
        Some(s) if s.status == Some(1) && s.name.is_some() => {
            let name = format!("Step {}", s.name.as_deref().unwrap_or("Plan"));
            let resets_at = s
                .expired_at
                .and_then(|s| s.parse::<f64>().ok())
                .and_then(epoch_to_iso);
            Ok(Some((name, resets_at)))
        }
        _ => Ok(None),
    }
}

fn parse_rate_limit(body: &str) -> Result<RateLimitWindows, VendorError> {
    let resp: StepPlanRateLimit = serde_json::from_str(body)
        .map_err(|e| VendorError::Parse(format!("stepfun rate-limit: {e}")))?;
    let plan = resp.plan_credit_rate_limit.unwrap_or_default();
    let five_h_epoch = epoch_opt(&resp.five_hour_usage_reset_time);
    let weekly_epoch = epoch_opt(&resp.weekly_usage_reset_time);
    Ok(RateLimitWindows {
        plan_used_pct: plan
            .subscription_credit_left_rate
            .map(left_rate_to_used_pct),
        plan_resets_at: plan
            .subscription_credit_reset_time
            .as_deref()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|v| *v > 0.0)
            .and_then(epoch_to_iso),
        // Only include 5h/windows when the reset_time is a positive epoch
        // ("0" means the plan doesn't have that window type).
        five_h_used_pct: five_h_epoch
            .map(|_| left_rate_to_used_pct(resp.five_hour_usage_left_rate.unwrap_or(0.0))),
        five_h_resets_at: five_h_epoch.and_then(epoch_to_iso),
        weekly_used_pct: weekly_epoch
            .map(|_| left_rate_to_used_pct(resp.weekly_usage_left_rate.unwrap_or(0.0))),
        weekly_resets_at: weekly_epoch.and_then(epoch_to_iso),
    })
}

fn build_quota(
    balance: f64,
    credit: f64,
    plan: Option<(String, Option<String>)>,
    limits: &RateLimitWindows,
) -> Quota {
    let (plan_label_raw, plan_expires_at) = plan.unwrap_or_default();
    let plan_label = if plan_label_raw.is_empty() && credit > 0.0 {
        "Step Plan".to_string()
    } else if plan_label_raw.is_empty() {
        "按量付费".to_string()
    } else {
        plan_label_raw
    };

    let has_plan = !plan_label.eq_ignore_ascii_case("按量付费");

    let mut windows: Vec<QuotaWindow> = Vec::new();

    // ── Step Plan subscription window ──
    // resets_at = the monthly credit rollover (subscription_credit_reset_time);
    // the PLAN expiry (expired_at) goes to Quota.expires_at instead.
    if has_plan {
        windows.push(QuotaWindow {
            label: plan_label.clone(),
            used_pct: limits.plan_used_pct.unwrap_or(0.0),
            resets_at: limits.plan_resets_at.clone(),
            ..Default::default()
        });
    }

    // ── 5-hour short-term window (only when present) ──
    if limits.five_h_used_pct.is_some() && limits.five_h_resets_at.is_some() {
        windows.push(QuotaWindow {
            label: "5h".into(),
            used_pct: limits.five_h_used_pct.unwrap_or(0.0),
            resets_at: limits.five_h_resets_at.clone(),
            ..Default::default()
        });
    }

    // ── Weekly window ──
    if limits.weekly_used_pct.is_some() && limits.weekly_resets_at.is_some() {
        windows.push(QuotaWindow {
            label: "周".into(),
            used_pct: limits.weekly_used_pct.unwrap_or(0.0),
            resets_at: limits.weekly_resets_at.clone(),
            ..Default::default()
        });
    }

    // Status: worst of all windows, or balance-derived.
    let status = QuotaStatus::worst_of(
        windows
            .iter()
            .map(|w| QuotaStatus::from_used_pct(w.used_pct)),
    );
    let status = if windows.is_empty() {
        if balance <= 0.0 && credit <= 0.0 {
            QuotaStatus::Danger
        } else {
            QuotaStatus::Ok
        }
    } else {
        status
    };

    Quota {
        site: None,
        vendor: "stepfun".into(),
        status,
        windows,
        balance: Some(QuotaBalance {
            amount: balance,
            currency: "CNY".into(),
            today_consumption: None,
            month_consumption: None,
        }),
        plan_label: if has_plan {
            Some(plan_label)
        } else {
            Some("按量付费".into())
        },
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: if has_plan { plan_expires_at } else { None },
    }
}

// ── Fetch ───────────────────────────────────────────────────────────────────

pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let raw_cookie = serde_json::from_str::<serde_json::Value>(credential)
        .ok()
        .and_then(|v| {
            v.get("cookie")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| credential.to_string());
    let cookie = normalize_cookie(&raw_cookie);
    let oasis_token = cookie_value(&cookie, "Oasis-Token")
        .ok_or_else(|| VendorError::Parse("Cookie 中缺少 Oasis-Token（未登录）".into()))?;
    let oasis_webid = cookie_value(&cookie, "Oasis-Webid")
        .ok_or_else(|| VendorError::Parse("Cookie 中缺少 Oasis-Webid".into()))?;

    // Only send the two essential cookies — extras (GA, tracking, etc.) can
    // interfere with the API. Oasis-Token has a ~6 month TTL so there is no
    // need for a session touch; the token remains valid across scheduler ticks.
    let cookie = format!("Oasis-Token={oasis_token}; Oasis-Webid={oasis_webid}");

    let call = |url: &str| -> Result<String, VendorError> {
        let body = http.connect_rpc(url, &cookie, &oasis_webid)?;
        // When the session cookie is stale, stepfun.com returns 200 with a
        // login-page HTML redirect instead of a proper HTTP 401. Detect that
        // early so the scheduler surfaces a cookie_error rather than a
        // generic parse failure → "额度读取待实现".
        let trimmed = body.trim();
        if trimmed.is_empty() || trimmed.starts_with('<') {
            return Err(VendorError::Auth("Cookie 已过期，请重新获取".into()));
        }
        Ok(body)
    };

    // 1. Balance (mandatory).
    let (balance, credit) = parse_balance(&call(BALANCE_URL)?)?;

    // 2. Step Plan subscription (optional).
    let plan = call(PLAN_URL)
        .ok()
        .and_then(|b| parse_plan(&b).ok().flatten());

    // 3. Usage rate-limit windows (optional).
    let limits = call(RATE_LIMIT_URL)
        .ok()
        .and_then(|b| parse_rate_limit(&b).ok())
        .unwrap_or_default();

    Ok(build_quota(balance, credit, plan, &limits))
}

pub async fn fetch(credential: &str) -> Result<Quota, VendorError> {
    let cred = credential.to_string();
    tokio::task::spawn_blocking(move || fetch_with(UreqHttp::instance(), &cred))
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
            agent: crate::utils::http::direct_agent_builder()
                .redirects(0)
                .build(),
        })
    }
}
impl Http for UreqHttp {
    fn connect_rpc(
        &self,
        url: &str,
        cookie: &str,
        oasis_webid: &str,
    ) -> Result<String, VendorError> {
        let resp = self
            .agent
            .post(url)
            .set("Content-Type", "application/json")
            .set("Accept", "*/*")
            .set("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .set("Connect-Protocol-Version", "1")
            .set("Cookie", cookie)
            .set("oasis-appid", OASIS_APPID)
            .set("oasis-platform", "web")
            .set("oasis-webid", oasis_webid)
            .set("Origin", "https://platform.stepfun.com")
            .set("Referer", "https://platform.stepfun.com/account-overview")
            .set(
                "User-Agent",
                "Mozilla/5.0 AppleWebKit/537.36 Chrome/150 Safari/537.36",
            )
            .send_string("{}");
        match resp {
            Ok(resp) => resp
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string())),
            Err(ureq::Error::Status(code, r)) => {
                if code == 401 || code == 403 || (300..400).contains(&code) {
                    return Err(VendorError::Network("status code 401".into()));
                }
                Err(VendorError::Api {
                    status: code,
                    body: r.into_string().unwrap_or_default(),
                })
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SAMPLE_BALANCE: &str =
        r#"{"voucher":"0","payment":"0","balance":"48.20","credit":"0","cost_month":"1.2"}"#;
    static SAMPLE_PLAN: &str = r#"{"status":1,"subscription":{"plan_type":1,"name":"Plus","status":1,"activated_at":"1783578343","expired_at":"1791354343"},"can_resign":true}"#;
    static SAMPLE_RATE_LIMIT: &str = r#"{"status":1,"five_hour_usage_left_rate":0,"five_hour_usage_reset_time":"0","weekly_usage_left_rate":0,"weekly_usage_reset_time":"0","plan_family":2,"plan_credit_rate_limit":{"subscription_credit_left_rate":0.9267404,"subscription_credit_reset_time":"1786170343","topup_credit_left_rate":0,"credit_buckets":[{"type":1,"credit_total":"1600000000","credit_residual":"1482784664","expire_at":"1791354343","next_reset_at":"1786170343"}]}}"#;

    #[test]
    fn parse_balance_ok() {
        let (b, c) = parse_balance(SAMPLE_BALANCE).unwrap();
        assert_eq!(b, 48.20);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn parse_plan_active() {
        let p = parse_plan(SAMPLE_PLAN).unwrap().expect("should be Some");
        assert_eq!(p.0, "Step Plus");
        assert!(p.1.is_some());
    }

    #[test]
    fn parse_rate_limit_ok() {
        let r = parse_rate_limit(SAMPLE_RATE_LIMIT).unwrap();
        let pct = r.plan_used_pct.expect("should have plan used pct");
        assert!((pct - 7.32).abs() < 0.1); // 1-0.92674 = 0.07326 * 100 ≈ 7.33
        assert!(r.plan_resets_at.is_some());
        // 5h / weekly not applicable (left_rate=0, reset_time="0")
        assert!(r.five_h_used_pct.is_none());
        assert!(r.weekly_used_pct.is_none());
    }

    #[test]
    fn build_full_step_plus() {
        let (balance, credit) = parse_balance(SAMPLE_BALANCE).unwrap();
        let plan = parse_plan(SAMPLE_PLAN).unwrap();
        let limits = parse_rate_limit(SAMPLE_RATE_LIMIT).unwrap();
        let q = build_quota(balance, credit, plan, &limits);
        assert_eq!(q.vendor, "stepfun");
        assert_eq!(q.plan_label.as_deref(), Some("Step Plus"));
        assert_eq!(q.balance.as_ref().unwrap().amount, 48.20);
        assert_eq!(q.windows.len(), 1); // only Step Plus (5h/周不适用)
        assert_eq!(q.windows[0].label, "Step Plus");
        assert!(q.windows[0].used_pct > 0.0);
        assert!(q.windows[0].resets_at.is_some());
        assert_eq!(q.status, QuotaStatus::Ok); // 7.33% used
    }

    #[test]
    fn build_paygo_no_plan() {
        let q = build_quota(48.20, 0.0, None, &RateLimitWindows::default());
        assert_eq!(q.plan_label.as_deref(), Some("按量付费"));
        assert_eq!(q.balance.as_ref().unwrap().amount, 48.20);
        assert!(q.windows.is_empty());
    }

    #[test]
    fn left_rate_conversion() {
        assert_eq!(left_rate_to_used_pct(1.0), 0.0);
        assert_eq!(left_rate_to_used_pct(0.5), 50.0);
        assert_eq!(left_rate_to_used_pct(0.0), 100.0);
    }

    // ── Mock Http for fetch_with tests ──────────────────────────────────
    struct MockHttp {
        rpc_body: String,
    }

    impl Http for MockHttp {
        fn connect_rpc(
            &self,
            _url: &str,
            _cookie: &str,
            _webid: &str,
        ) -> Result<String, VendorError> {
            if self.rpc_body.is_empty() {
                Err(VendorError::Network("mock error".into()))
            } else {
                Ok(self.rpc_body.clone())
            }
        }
    }

    #[test]
    fn fetch_with_minimal_cookie_extracts_token_and_webid() {
        let mock = MockHttp {
            rpc_body: SAMPLE_BALANCE.to_string(),
        };
        // Realistic user-pasted cookie with extra noise (GA, tracking, etc.).
        let cred = r#"{"cookie":"_ga=GA1.1.123; Oasis-Token=my-token; _gid=456; Oasis-Webid=my-webid; _gat=1"}"#;
        let result = fetch_with(&mock, cred).unwrap();
        assert!(result.balance.is_some());
    }

    #[test]
    fn fetch_with_minimal_cookie_rejects_missing_token() {
        let mock = MockHttp {
            rpc_body: String::new(),
        };
        let cred = r#"{"cookie":"Oasis-Webid=web; OTHER=val"}"#;
        assert!(fetch_with(&mock, cred).is_err());
    }
}
