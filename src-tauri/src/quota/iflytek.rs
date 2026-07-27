//! iFlytek 星辰 (Astron) MaaS coding-plan adapter — cookie-based console API.
//!
//! Auth is via the `maas.xfyun.cn` console session Cookie (must contain
//! `ssoSessionId`; the whole cookie string is replayed).
//!
//! GET /api/v1/gpt-finetune/coding-plan/list?page=1&size=6
//!   → `data.rows[]` — each an active coding plan carrying:
//!     - `name`       plan tier ("专业版")
//!     - `expiresAt`  plan expiry ("YYYY-MM-DD HH:MM:SS", China Standard Time)
//!     - `status`     2 = active
//!     - `codingPlanUsageDTO` daily / package / 5h / weekly usage counters
//!
//! The plan tier becomes `plan_label`; a plan window carries the usage % + the
//! real expiry. Optional 5h / weekly windows appear when the plan defines them.
//! Balance is fetched separately (TODO: pending the balance endpoint).

use serde::Deserialize;

use super::types::{Quota, QuotaBalance, QuotaStatus, QuotaWindow};
use super::VendorError;

const PLAN_URL: &str = "https://maas.xfyun.cn/api/v1/gpt-finetune/coding-plan/list?page=1&size=6";
const BALANCE_URL: &str = "https://maas.xfyun.cn/api/v1/gpt-finetune/user/balance";

pub trait Http {
    fn get_with_cookie(&self, url: &str, cookie: &str) -> Result<String, VendorError>;
}

// ── API response types ──────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct PlanListResp {
    /// Envelope code: 0 = success, non-zero = error (often auth-related when
    /// the session cookie is stale — the API still returns HTTP 200).
    #[serde(default)]
    code: i64,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    data: Option<PlanData>,
}
#[derive(Debug, Default, Deserialize)]
struct PlanData {
    #[serde(default)]
    rows: Vec<PlanRow>,
}
#[derive(Debug, Default, Deserialize)]
#[allow(non_snake_case)]
struct PlanRow {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    expiresAt: Option<String>,
    #[serde(default)]
    status: Option<i64>,
    #[serde(default)]
    codingPlanUsageDTO: Option<UsageDto>,
}
#[derive(Debug, Default, Deserialize)]
#[allow(non_snake_case)]
struct UsageDto {
    #[serde(default)]
    dailyLimit: Option<f64>,
    #[serde(default)]
    dailyUsage: Option<f64>,
    #[serde(default)]
    packageLimit: Option<f64>,
    #[serde(default)]
    packageUsage: Option<f64>,
    #[serde(default)]
    rp5hLimit: Option<f64>,
    #[serde(default)]
    rp5hUsage: Option<f64>,
    #[serde(default)]
    rpwLimit: Option<f64>,
    #[serde(default)]
    rpwUsage: Option<f64>,
}

/// `/user/balance` response: `data.balance` is the account balance in CNY;
/// `virtualBalance` is bonus/gift credit.
#[derive(Debug, Default, Deserialize)]
struct BalanceResp {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    data: Option<BalanceData>,
}
#[derive(Debug, Default, Deserialize)]
#[allow(non_snake_case)]
struct BalanceData {
    #[serde(default)]
    balance: Option<f64>,
    #[serde(default)]
    virtualBalance: Option<f64>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// used% = usage / limit, only when limit is a positive number.
fn pct(usage: Option<f64>, limit: Option<f64>) -> Option<f64> {
    match (usage, limit) {
        (u, Some(l)) if l > 0.0 => Some((u.unwrap_or(0.0) / l * 100.0).clamp(0.0, 100.0)),
        _ => None,
    }
}

/// Parse iFlytek's "YYYY-MM-DD HH:MM:SS" (China Standard Time, UTC+8) into a
/// normalized RFC3339 UTC string.
fn cst_to_utc_iso(s: &str) -> Option<String> {
    let s = s.trim();
    let dt = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
    let tz = chrono::FixedOffset::east_opt(8 * 3600)?;
    dt.and_local_timezone(tz)
        .single()
        .map(|t| t.with_timezone(&chrono::Utc).to_rfc3339())
}

fn normalize_cookie(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix("Cookie:")
        .or_else(|| raw.strip_prefix("cookie:"))
        .map(str::trim)
        .unwrap_or(raw);
    raw.to_string()
}

fn cookie_has(cookie: &str, name: &str) -> bool {
    let prefix = format!("{name}=");
    cookie.split(';').any(|p| p.trim().starts_with(&prefix))
}

/// True when `expires_at` (CST "YYYY-MM-DD HH:MM:SS") is at/before `now_utc`.
/// Rows without a parseable expiry are treated as NOT expired (keep them).
fn is_expired(expires_at: &Option<String>, now_utc: chrono::DateTime<chrono::Utc>) -> bool {
    match expires_at.as_deref().and_then(cst_to_utc_iso) {
        Some(iso) => match chrono::DateTime::parse_from_rfc3339(&iso) {
            Ok(dt) => dt.with_timezone(&chrono::Utc) <= now_utc,
            Err(_) => false,
        },
        None => false,
    }
}

/// Pick the most relevant NON-EXPIRED plan row: drop expired plans, prefer
/// active (status==2), then the furthest expiry.
/// Comparator is strict: primary key = active status, secondary = expiry date.
/// This avoids the non-transitive cmp bug where (active, early) vs (inactive,
/// late) could be ordered differently depending on argument order.
fn pick_row(rows: Vec<PlanRow>, now_utc: chrono::DateTime<chrono::Utc>) -> Option<PlanRow> {
    rows.into_iter()
        .filter(|r| r.name.is_some() && !is_expired(&r.expiresAt, now_utc))
        .max_by(|a, b| {
            let sa = a.status == Some(2);
            let sb = b.status == Some(2);
            match sa.cmp(&sb) {
                std::cmp::Ordering::Equal => a.expiresAt.cmp(&b.expiresAt),
                other => other,
            }
        })
}

// ── Parse ─────────────────────────────────────────────────────────────────

/// Parse `/user/balance` → total available balance (balance + virtual) in CNY.
/// Returns `None` when the body is unparseable so the plan data still renders.
/// Returns `Err(Auth)` when the envelope reports a non-zero code — the console
/// API returns HTTP 200 with an error code (often auth) when the session cookie
/// is stale, and we must not mistake that for "no balance".
fn parse_balance(body: &str) -> Result<Option<QuotaBalance>, VendorError> {
    let resp: BalanceResp = serde_json::from_str(body)
        .map_err(|e| VendorError::Parse(format!("iflytek balance: {e}")))?;
    if resp.code != 0 {
        return Err(VendorError::Auth(format!(
            "balance code {} ({})",
            resp.code,
            resp.msg.unwrap_or_default()
        )));
    }
    let Some(data) = resp.data else {
        return Ok(None);
    };
    let amount = data.balance.unwrap_or(0.0) + data.virtualBalance.unwrap_or(0.0);
    Ok(Some(QuotaBalance {
        amount,
        currency: "CNY".into(),
        today_consumption: None,
        month_consumption: None,
    }))
}

/// Parse the coding-plan list into a [`Quota`]. `balance` is fetched separately
/// and attached (None when the balance call failed or wasn't made). Expired
/// plans (`expiresAt` ≤ `now_utc`) are skipped.
pub fn parse(
    body: &str,
    balance: Option<QuotaBalance>,
    now_utc: chrono::DateTime<chrono::Utc>,
) -> Result<Quota, VendorError> {
    let resp: PlanListResp =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(format!("iflytek plan: {e}")))?;
    // Envelope code != 0 → the console API rejected the request (most often a
    // stale session cookie). Surface as auth so the scheduler shows cookie_error.
    if resp.code != 0 {
        return Err(VendorError::Auth(format!(
            "plan code {} ({})",
            resp.code,
            resp.msg.unwrap_or_default()
        )));
    }
    let rows = resp.data.map(|d| d.rows).unwrap_or_default();
    let row = match pick_row(rows, now_utc) {
        Some(r) => r,
        // No active (non-expired) plan — still surface balance-only if present.
        None => {
            let balance = balance.ok_or(VendorError::Empty)?;
            return Ok(Quota {
                vendor: "iflytek".into(),
                status: QuotaStatus::Ok,
                windows: vec![],
                balance: Some(balance),
                plan_label: Some("按量付费".into()),
                refreshed_at: None,
                error: None,
                cookie_error: None,
                expires_at: None,
            });
        }
    };

    let plan_label = row.name.clone().unwrap_or_else(|| "套餐".into());
    // Plan expiry → Quota.expires_at (the "到期" tag). The daily/package quota
    // resets on its own cadence with no per-window timestamp, so resets_at=None.
    let expires_at = row.expiresAt.as_deref().and_then(cst_to_utc_iso);
    let usage = row.codingPlanUsageDTO.unwrap_or_default();

    // Primary plan usage: package quota if defined, else the daily limit.
    let primary_used = pct(usage.packageUsage, usage.packageLimit)
        .or_else(|| pct(usage.dailyUsage, usage.dailyLimit))
        .unwrap_or(0.0);

    let mut windows: Vec<QuotaWindow> = vec![QuotaWindow {
        label: plan_label.clone(),
        used_pct: primary_used,
        resets_at: None,
    }];

    // Optional short-term rate-limit windows (present only for some plans).
    if let Some(p) = pct(usage.rp5hUsage, usage.rp5hLimit) {
        windows.push(QuotaWindow {
            label: "5h".into(),
            used_pct: p,
            resets_at: None,
        });
    }
    if let Some(p) = pct(usage.rpwUsage, usage.rpwLimit) {
        windows.push(QuotaWindow {
            label: "周".into(),
            used_pct: p,
            resets_at: None,
        });
    }

    let status = QuotaStatus::worst_of(
        windows
            .iter()
            .map(|w| QuotaStatus::from_used_pct(w.used_pct)),
    );

    Ok(Quota {
        vendor: "iflytek".into(),
        status,
        windows,
        balance,
        plan_label: Some(plan_label),
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at,
    })
}

// ── Fetch ─────────────────────────────────────────────────────────────────

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
    if cookie.is_empty() {
        return Err(VendorError::Parse(
            "缺少 Cookie（需含 ssoSessionId）".into(),
        ));
    }
    if !cookie_has(&cookie, "ssoSessionId") {
        return Err(VendorError::Parse(
            "Cookie 中缺少 ssoSessionId（未登录）".into(),
        ));
    }

    // Balance — propagate auth errors (401/403) so the scheduler surfaces
    // cookie_error instead of silently showing an empty card.
    let balance = fetch_balance(http, &cookie)?;

    let body = http.get_with_cookie(PLAN_URL, &cookie)?;
    parse(&body, balance, chrono::Utc::now())
}

/// Fetch balance, propagating auth errors. Network errors return None so
/// the plan data can still render (balance is optional).
fn fetch_balance(http: &dyn Http, cookie: &str) -> Result<Option<QuotaBalance>, VendorError> {
    match http.get_with_cookie(BALANCE_URL, cookie) {
        Ok(body) => parse_balance(&body),
        Err(VendorError::Network(msg)) if msg.contains("status code 401") => {
            // Cookie expired → surface as auth error so the scheduler writes
            // a placeholder with cookie_error for the frontend to display.
            Err(VendorError::Network(msg))
        }
        Err(_) => Ok(None),
    }
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
            agent: ureq::AgentBuilder::new().redirects(0).build(),
        })
    }
}
impl Http for UreqHttp {
    fn get_with_cookie(&self, url: &str, cookie: &str) -> Result<String, VendorError> {
        let resp = self
            .agent
            .get(url)
            .set("Accept", "application/json, text/plain, */*")
            .set("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .set("Cookie", cookie)
            .set("Referer", "https://maas.xfyun.cn/packageSubscription")
            .set(
                "User-Agent",
                "Mozilla/5.0 AppleWebKit/537.36 Chrome/150 Safari/537.36",
            )
            .call();
        match resp {
            Ok(r) => r
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

    /// A fixed "now" before all sample expiries (2026-04-01), so non-expired
    /// plans are kept. Tests that check expiry-skipping pass a later time.
    fn now_early() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-04-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    // Real /coding-plan/list response (trimmed modelInfo array).
    static SAMPLE: &str = r#"{"code":0,"data":{"page":1,"rows":[{"appId":"mcce7a42","codingPlanUsageDTO":{"appId":"mcce7a42","channel":"astron-code-latest","dailyLimit":20000000,"dailyUsage":0,"packageLeft":null,"packageLimit":null,"packageUsage":null,"rp5hLimit":null,"rp5hUsage":null,"rpwLimit":null,"rpwUsage":null},"expiresAt":"2026-04-18 16:04:00","name":"专业版","status":2,"validFrom":"2026-03-18 16:04:51"}],"size":6,"total":1},"succeed":true}"#;

    #[test]
    fn parse_professional_plan() {
        let q = parse(SAMPLE, None, now_early()).unwrap();
        assert_eq!(q.vendor, "iflytek");
        assert_eq!(q.plan_label.as_deref(), Some("专业版"));
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "专业版");
        // daily 0 / 20000000 = 0% used
        assert!((q.windows[0].used_pct - 0.0).abs() < 1e-6);
        // Plan window has NO per-window reset (daily quota, no timestamp).
        assert!(q.windows[0].resets_at.is_none());
        // Plan expiry goes to Quota.expires_at (CST → UTC): 2026-04-18 08:04Z.
        let iso = q.expires_at.as_deref().unwrap();
        assert!(iso.starts_with("2026-04-18T08:04"));
        assert_eq!(q.status, QuotaStatus::Ok);
        assert!(q.balance.is_none());
    }

    #[test]
    fn parse_with_package_and_windows() {
        let body = r#"{"code":0,"data":{"rows":[{"name":"旗舰版","expiresAt":"2026-12-31 23:59:59","status":2,"codingPlanUsageDTO":{"packageLimit":1000,"packageUsage":850,"rp5hLimit":100,"rp5hUsage":60,"rpwLimit":500,"rpwUsage":100}}]}}"#;
        let q = parse(body, None, now_early()).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("旗舰版"));
        assert_eq!(q.windows.len(), 3);
        // package 850/1000 = 85% → the plan window
        assert!((q.windows[0].used_pct - 85.0).abs() < 1e-6);
        // 5h 60% + weekly 20%
        assert_eq!(q.windows[1].label, "5h");
        assert!((q.windows[1].used_pct - 60.0).abs() < 1e-6);
        assert_eq!(q.windows[2].label, "周");
        assert!((q.windows[2].used_pct - 20.0).abs() < 1e-6);
        // worst = 85% → Danger
        assert_eq!(q.status, QuotaStatus::Danger);
        assert!(q.expires_at.as_deref().unwrap().starts_with("2026-12-31"));
    }

    #[test]
    fn pick_row_prefers_active_then_latest_expiry() {
        let body = r#"{"data":{"rows":[
            {"name":"体验版","expiresAt":"2026-01-01 00:00:00","status":3},
            {"name":"专业版","expiresAt":"2026-06-01 00:00:00","status":2},
            {"name":"基础版","expiresAt":"2026-03-01 00:00:00","status":2}
        ]}}"#;
        // now = 2026-02-15: 基础版(2026-03) and 专业版(2026-06) still valid,
        // 体验版(2026-01) expired. Among active, furthest expiry (专业版) wins.
        let now = chrono::DateTime::parse_from_rfc3339("2026-02-15T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let q = parse(body, None, now).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("专业版"));
    }

    #[test]
    fn parse_skips_all_expired_plans() {
        // Both plans expired by 2027 → no plan; falls back to balance (paygo)
        // or Empty when no balance.
        let body = r#"{"data":{"rows":[
            {"name":"专业版","expiresAt":"2026-04-18 16:04:00","status":2},
            {"name":"体验版","expiresAt":"2026-01-01 00:00:00","status":3}
        ]}}"#;
        let now = chrono::DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        // No balance → Empty (nothing to show).
        assert!(matches!(parse(body, None, now), Err(VendorError::Empty)));
        // With balance → pay-as-you-go card, no plan window.
        let bal = QuotaBalance {
            amount: 10.0,
            currency: "CNY".into(),
            today_consumption: None,
            month_consumption: None,
        };
        let q = parse(body, Some(bal), now).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("按量付费"));
        assert!(q.windows.is_empty());
        assert!(q.expires_at.is_none());
    }

    #[test]
    fn parse_balance_sums_real_and_virtual() {
        let b = parse_balance(
            r#"{"code":0,"data":{"balance":12.5,"delinquent":0,"virtualBalance":3.5}}"#,
        )
        .expect("should parse")
        .expect("should have balance");
        assert!((b.amount - 16.0).abs() < 1e-6);
        assert_eq!(b.currency, "CNY");
    }

    #[test]
    fn parse_zero_balance() {
        let b =
            parse_balance(r#"{"code":0,"data":{"balance":0,"delinquent":0,"virtualBalance":0}}"#)
                .expect("should parse")
                .expect("should have balance");
        assert_eq!(b.amount, 0.0);
    }

    #[test]
    fn parse_balance_nonzero_code_is_auth_error() {
        // Console API returns HTTP 200 with a non-zero code when the session
        // cookie is stale — must surface as Auth, not None.
        let err = parse_balance(r#"{"code":401,"msg":"未登录","data":null}"#)
            .expect_err("non-zero code should error");
        assert!(matches!(err, VendorError::Auth(_)));
    }

    #[test]
    fn parse_balance_missing_data_returns_none() {
        let b = parse_balance(r#"{"code":0,"data":null}"#)
            .expect("should parse");
        assert!(b.is_none());
    }

    #[test]
    fn parse_plan_nonzero_code_is_auth_error() {
        // Stale session → console returns 200 with code != 0. Must surface as
        // Auth so the scheduler shows a cookie_error (not a bare empty card).
        let err = parse(
            r#"{"code":401,"msg":"请先登录","data":null}"#,
            None,
            now_early(),
        )
        .expect_err("non-zero plan code should error");
        assert!(matches!(err, VendorError::Auth(_)));
    }

    #[test]
    fn parse_attaches_balance_and_no_plan_is_paygo() {
        let bal = QuotaBalance {
            amount: 48.2,
            currency: "CNY".into(),
            today_consumption: None,
            month_consumption: None,
        };
        // Plan present → balance attached alongside windows.
        let q = parse(SAMPLE, Some(bal.clone()), now_early()).unwrap();
        assert_eq!(q.balance.as_ref().unwrap().amount, 48.2);
        assert_eq!(q.plan_label.as_deref(), Some("专业版"));
        // No plan rows but balance present → pay-as-you-go card.
        let q2 = parse(r#"{"data":{"rows":[]}}"#, Some(bal), now_early()).unwrap();
        assert_eq!(q2.plan_label.as_deref(), Some("按量付费"));
        assert!(q2.windows.is_empty());
        assert_eq!(q2.balance.as_ref().unwrap().amount, 48.2);
    }

    #[test]
    fn parse_empty_rows_no_balance_errors() {
        assert!(matches!(
            parse(r#"{"code":0,"data":{"rows":[]}}"#, None, now_early()),
            Err(VendorError::Empty)
        ));
    }

    #[test]
    fn fetch_with_requires_sso_session() {
        // Plan with a far-future expiry so it's never skipped by the real
        // `Utc::now()` used inside fetch_with.
        static FUTURE_PLAN: &str = r#"{"code":0,"data":{"rows":[{"name":"专业版","expiresAt":"2099-12-31 23:59:59","status":2,"codingPlanUsageDTO":{"dailyLimit":100,"dailyUsage":0}}]}}"#;
        struct Mock;
        impl Http for Mock {
            fn get_with_cookie(&self, url: &str, _: &str) -> Result<String, VendorError> {
                if url.contains("/user/balance") {
                    return Ok(r#"{"code":0,"data":{"balance":10,"virtualBalance":0}}"#.to_string());
                }
                Ok(FUTURE_PLAN.to_string())
            }
        }
        // Missing ssoSessionId → rejected before any HTTP call.
        assert!(fetch_with(&Mock, r#"{"cookie":"foo=1; bar=2"}"#).is_err());
        // With ssoSessionId → parses plan + balance.
        let q = fetch_with(&Mock, r#"{"cookie":"ssoSessionId=abc; account_id=1"}"#).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("专业版"));
        assert_eq!(q.balance.as_ref().unwrap().amount, 10.0);
    }

    // ── pick_row comparator strictness ──────────────────────────────────────

    #[test]
    fn pick_row_active_wins_over_inactive_even_when_earlier_expiry() {
        // Active plan expires 2026-03-01, inactive expires 2026-12-31.
        // Old (broken) comparator: (true,"2026-03-01").cmp(&(false,"2026-12-31"))
        // = Greater → active wins. But (false,"2026-12-31").cmp(&(true,"2026-03-01"))
        // = Less → inconsistent → max_by UB, could pick inactive.
        // Fixed comparator: status is primary key → active always wins.
        let body = r#"{"data":{"rows":[
            {"name":"基础版","expiresAt":"2026-03-01 00:00:00","status":2},
            {"name":"过期版","expiresAt":"2026-12-31 23:59:59","status":3}
        ]}}"#;
        let now = chrono::DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let q = parse(body, None, now).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("基础版"));
    }

    #[test]
    fn pick_row_multiple_active_picks_latest_expiry() {
        let body = r#"{"data":{"rows":[
            {"name":"专业版","expiresAt":"2026-06-01 00:00:00","status":2},
            {"name":"基础版","expiresAt":"2026-03-01 00:00:00","status":2},
            {"name":"旗舰版","expiresAt":"2026-12-01 00:00:00","status":2}
        ]}}"#;
        let now = chrono::DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let q = parse(body, None, now).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("旗舰版"));
    }

    // ── balance API auth error propagation ──────────────────────────────────

    #[test]
    fn fetch_with_balance_401_propagates_as_auth_error() {
        struct Mock401;
        impl Http for Mock401 {
            fn get_with_cookie(&self, url: &str, _: &str) -> Result<String, VendorError> {
                if url.contains("/user/balance") {
                    return Err(VendorError::Network("status code 401".into()));
                }
                Ok(r#"{"code":0,"data":{"rows":[]}}"#.to_string())
            }
        }
        let cred = r#"{"cookie":"ssoSessionId=abc; account_id=1"}"#;
        let result = fetch_with(&Mock401, cred);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("401"),
            "expected auth error, got: {err_msg}"
        );
    }

    #[test]
    fn fetch_with_balance_network_error_ignored_plan_still_renders() {
        static FUTURE_PLAN: &str = r#"{"code":0,"data":{"rows":[{"name":"专业版","expiresAt":"2099-12-31 23:59:59","status":2,"codingPlanUsageDTO":{"dailyLimit":100,"dailyUsage":0}}]}}"#;
        struct MockNetFail;
        impl Http for MockNetFail {
            fn get_with_cookie(&self, url: &str, _: &str) -> Result<String, VendorError> {
                if url.contains("/user/balance") {
                    return Err(VendorError::Network("connection refused".into()));
                }
                Ok(FUTURE_PLAN.to_string())
            }
        }
        let cred = r#"{"cookie":"ssoSessionId=abc; account_id=1"}"#;
        let q = fetch_with(&MockNetFail, cred).unwrap();
        // Plan data still renders even though balance fetch failed.
        assert_eq!(q.plan_label.as_deref(), Some("专业版"));
        assert!(q.balance.is_none());
        assert_eq!(q.windows.len(), 1);
    }
}
