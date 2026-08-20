//! GLM / Z.ai Coding Plan adapter.
//!
//! Credential is a JSON blob `{ "key": "...", "region": "global" | "bigmodel-cn" }`
//! (Account.svelte serializes multi-field vendors).
//!   GET {base}/api/monitor/usage/quota/limit   → data.limits[] (windows)
//! `limits[]` splits into TOKENS_LIMIT entries (sorted by window length → the
//! shortest ≤6h is the 5-hour session, the longest is Weekly) and a single
//! TIME_LIMIT entry (the MCP monthly bucket). Each window carries its own
//! `nextResetTime` reset marker.
//!
//! Faithfully ported from token-monitor src/shared/zaiLimits.js:
//!   - used% = (usage - remaining) / usage, maxed with currentValue, else the
//!     explicit percentage/usedPercent field.
//!   - resetsAt = limit.nextResetTime | next_reset_time (epoch or ISO).

use serde::Deserialize;

use super::types::{epoch_to_iso, parse_iso, Quota, QuotaStatus, QuotaWindow};
use super::VendorError;
use chrono::NaiveDateTime;

const BASE_GLOBAL: &str = "https://api.z.ai";
const BASE_CN: &str = "https://open.bigmodel.cn";
const QUOTA_PATH: &str = "/api/monitor/usage/quota/limit";
const SUBSCRIPTION_PATH: &str = "/api/biz/subscription/list";

/// Parse the `{ key, region }` credential blob.
#[derive(Debug, Deserialize)]
struct Credential {
    key: String,
    #[serde(default)]
    region: Option<String>,
}

/// One entry in `data.limits[]`.
#[derive(Debug, Deserialize)]
struct Limit {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    unit: Option<f64>,
    #[serde(default)]
    number: Option<f64>,
    /// Total budget for this window (zaiUsedPercent reads `usage` as the total).
    #[serde(default)]
    usage: Option<f64>,
    #[serde(default)]
    remaining: Option<f64>,
    #[serde(default, rename = "currentValue", alias = "current_value")]
    current_value: Option<f64>,
    #[serde(default)]
    percentage: Option<f64>,
    #[serde(default, rename = "usedPercent", alias = "used_percent")]
    used_percent: Option<f64>,
    /// Per-window reset marker (epoch seconds/ms or ISO string).
    #[serde(default, rename = "nextResetTime", alias = "next_reset_time")]
    next_reset_time: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct QuotaResp {
    #[serde(default)]
    data: QuotaData,
}
#[derive(Debug, Default, Deserialize)]
#[allow(non_snake_case)]
struct QuotaData {
    #[serde(default)]
    limits: Vec<Limit>,
    // Fallback plan fields (used when subscription API fails)
    #[serde(default)]
    plan_name: Option<String>,
    #[serde(default)]
    planName: Option<String>,
    #[serde(default)]
    package_name: Option<String>,
    #[serde(default)]
    packageName: Option<String>,
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    plan_type: Option<String>,
    #[serde(default)]
    planType: Option<String>,
    #[serde(default)]
    level: Option<String>,
}

/// Subscription API response (token-monitor zaiLimits.js).
#[derive(Debug, Deserialize)]
struct SubscriptionResp {
    #[serde(default)]
    data: Vec<SubscriptionRow>,
}
#[derive(Debug, Default, Deserialize)]
#[allow(non_snake_case)]
struct SubscriptionRow {
    #[serde(default)]
    product_name: Option<String>,
    #[serde(default)]
    productName: Option<String>,
    #[serde(default)]
    plan_name: Option<String>,
    #[serde(default)]
    planName: Option<String>,
    #[serde(default)]
    package_name: Option<String>,
    #[serde(default)]
    packageName: Option<String>,
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    plan_type: Option<String>,
    #[serde(default)]
    planType: Option<String>,
    #[serde(default)]
    level: Option<String>,
    /// 下次续费日期（YYYY-MM-DD）。连续多订阅取最大值作为整体到期。
    #[serde(default)]
    nextRenewTime: Option<String>,
    /// 订阅状态：VALID 表示生效中
    #[serde(default)]
    status: Option<String>,
}

/// Format plan text: normalize case, clean separators (token-monitor displayPlanText).
fn display_plan_text(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    // Replace underscores/hyphens with spaces, collapse runs.
    let spaced = s
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    // Apply regex-equivalent transformations matching the JS version:
    //   .replace(/\bglm\b/gi, 'GLM')
    //   .replace(/\bz\.?ai\b/gi, 'Z.ai')
    //   .replace(/\b\w/g, char => char.toUpperCase())
    //   .replace(/\bZ\.Ai\b/g, 'Z.ai')
    let processed = spaced
        .split_whitespace()
        .map(|word| {
            let lower = word.to_lowercase();
            if lower == "glm" {
                "GLM".to_string()
            } else if lower == "z.ai" || lower == "zai" {
                "Z.ai".to_string()
            } else {
                // Uppercase first character of the word only (JS \b\w).
                let mut chars = word.chars();
                match chars.next() {
                    Some(c) => {
                        let first = c.to_ascii_uppercase();
                        let mut result = String::with_capacity(word.len());
                        result.push(first);
                        result.push_str(chars.as_str());
                        result
                    }
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Fix any "Z.Ai" back to "Z.ai"
    processed.replace("Z.Ai", "Z.ai")
}

/// Extract plan name from subscription response, falling back to quota data fields.
fn extract_plan(quota_body: &str, sub_body: &str) -> Option<String> {
    // Try subscription first.
    if let Ok(sub) = serde_json::from_str::<SubscriptionResp>(sub_body) {
        let first = sub.data.into_iter().find(|r| {
            r.product_name.is_some()
                || r.productName.is_some()
                || r.plan_name.is_some()
                || r.planName.is_some()
                || r.package_name.is_some()
                || r.packageName.is_some()
                || r.plan.is_some()
                || r.plan_type.is_some()
                || r.planType.is_some()
                || r.level.is_some()
        });
        if let Some(row) = first {
            for v in [
                row.product_name,
                row.productName,
                row.plan_name,
                row.planName,
                row.package_name,
                row.packageName,
                row.plan,
                row.plan_type,
                row.planType,
                row.level,
            ]
            .into_iter()
            .flatten()
            {
                if !v.is_empty() {
                    return Some(display_plan_text(&v));
                }
            }
        }
    }
    // Fallback to quota data fields.
    if let Ok(resp) = serde_json::from_str::<QuotaResp>(quota_body) {
        for v in [
            resp.data.planName,
            resp.data.plan_name,
            resp.data.packageName,
            resp.data.package_name,
            resp.data.plan,
            resp.data.plan_type,
            resp.data.planType,
            resp.data.level,
        ]
        .into_iter()
        .flatten()
        {
            if !v.is_empty() {
                return Some(display_plan_text(&v));
            }
        }
    }
    None
}

/// Extract subscription plan name and the latest expiry date from `sub_body`.
/// Returns `(plan_label, resets_at_iso)`.
fn extract_subscription(quota_body: &str, sub_body: &str) -> (Option<String>, Option<String>) {
    let plan = extract_plan(quota_body, sub_body);
    let mut latest_expiry: Option<String> = None;

    if let Ok(sub) = serde_json::from_str::<SubscriptionResp>(sub_body) {
        for row in sub.data {
            // Only consider VALID (active) subscriptions.
            if row.status.as_deref() != Some("VALID") {
                continue;
            }
            // GLM subscriptions stack consecutively; the overall plan expiry
            // is the LATEST nextRenewTime across all VALID rows.
            if let Some(ref s) = row.nextRenewTime {
                // nextRenewTime is date-only ("YYYY-MM-DD"); append midnight so
                // NaiveDateTime always parses.
                let s_with_time = if s.contains(' ') {
                    s.to_string()
                } else {
                    format!("{s} 00:00:00")
                };
                if let Ok(dt) = NaiveDateTime::parse_from_str(&s_with_time, "%Y-%m-%d %H:%M:%S") {
                    let tz = chrono::FixedOffset::east_opt(8 * 3600)
                        .unwrap_or(chrono::FixedOffset::east_opt(0).unwrap());
                    let iso = dt.and_local_timezone(tz).single().map(|t| t.to_rfc3339());
                    if latest_expiry.is_none()
                        || iso.as_deref().unwrap_or("") > latest_expiry.as_deref().unwrap_or("")
                    {
                        latest_expiry = iso;
                    }
                }
            }
        }
    }

    (plan, latest_expiry)
}

/// HTTP client (GET + Bearer). Injected for unit tests.
pub trait Http {
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError>;
}

/// Window minutes per token-monitor zaiWindowMinutes: unit=5→minutes, 3→hours, 1→days, 6→weeks.
fn window_minutes(unit: Option<f64>, number: Option<f64>) -> Option<f64> {
    let (u, n) = (unit?, number?);
    if n <= 0.0 {
        return None;
    }
    Some(match u as i64 {
        5 => n,
        3 => n * 60.0,
        1 => n * 1440.0,
        6 => n * 10080.0,
        _ => return None,
    })
}

/// used% per zaiUsedPercent: prefer (usage - remaining)/usage (maxed with
/// currentValue), else the explicit percentage/usedPercent field.
fn limit_used_pct(l: &Limit) -> Option<f64> {
    if let Some(total) = l.usage {
        if total > 0.0 {
            let used_raw = match (l.remaining, l.current_value) {
                (Some(r), Some(cv)) => Some((total - r).max(cv)),
                (Some(r), None) => Some(total - r),
                (None, Some(cv)) => Some(cv),
                (None, None) => None,
            };
            if let Some(used) = used_raw {
                let used = used.clamp(0.0, total);
                return Some((used / total * 100.0).clamp(0.0, 100.0));
            }
        }
    }
    // Try explicit percentage fields first (prefer percentage, then used_percent alias).
    if let Some(p) = l.percentage.or(l.used_percent) {
        return Some(p.clamp(0.0, 100.0));
    }
    None
}

/// Reset marker (epoch number or ISO string) → RFC3339, if present.
fn reset_iso(v: &Option<serde_json::Value>) -> Option<String> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().and_then(epoch_to_iso),
        Some(serde_json::Value::String(s)) => parse_iso(s),
        _ => None,
    }
}

/// True when this TOKENS_LIMIT window is the ≤6h session window.
fn is_session_token_limit(l: &Limit) -> bool {
    matches!(window_minutes(l.unit, l.number), Some(m) if m <= 6.0 * 60.0)
}

/// Parse the quota-limit response body into all windows (5h / weekly / MCP monthly).
pub fn parse(body: &str) -> Result<Quota, VendorError> {
    let resp: QuotaResp =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;

    // Split into TOKENS_LIMIT (windows) and the single TIME_LIMIT (MCP monthly).
    let mut token_idx: Vec<usize> = Vec::new();
    let mut time_idx: Option<usize> = None;
    for (i, l) in resp.data.limits.iter().enumerate() {
        if limit_used_pct(l).is_none() {
            continue;
        }
        if l.kind.eq_ignore_ascii_case("TOKENS_LIMIT") {
            token_idx.push(i);
        } else if l.kind.eq_ignore_ascii_case("TIME_LIMIT") && time_idx.is_none() {
            time_idx = Some(i);
        }
    }

    // Sort token windows by ascending window length (shortest = session).
    token_idx.sort_by(|&a, &b| {
        let ma = window_minutes(resp.data.limits[a].unit, resp.data.limits[a].number)
            .unwrap_or(f64::MAX);
        let mb = window_minutes(resp.data.limits[b].unit, resp.data.limits[b].number)
            .unwrap_or(f64::MAX);
        ma.partial_cmp(&mb).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Resolve session vs weekly (matches zaiLimits sessionTokenLimit/tokenLimit).
    let (session, weekly): (Option<usize>, Option<usize>) = if token_idx.len() >= 2 {
        (Some(token_idx[0]), Some(token_idx[token_idx.len() - 1]))
    } else if let Some(&only) = token_idx.first() {
        if is_session_token_limit(&resp.data.limits[only]) {
            (Some(only), None)
        } else {
            (None, Some(only))
        }
    } else {
        (None, None)
    };

    let mut windows: Vec<QuotaWindow> = Vec::new();
    let mut push = |idx: usize, label: &str| {
        let l = &resp.data.limits[idx];
        if let Some(pct) = limit_used_pct(l) {
            windows.push(QuotaWindow {
                label: label.into(),
                used_pct: pct,
                resets_at: reset_iso(&l.next_reset_time),
                ..Default::default()
            });
        }
    };
    if let Some(i) = session {
        push(i, "5h");
    }
    if let Some(i) = weekly {
        push(i, "周");
    }
    if let Some(i) = time_idx {
        push(i, "MCP 月");
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
        vendor: "glm".into(),
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

/// Fetch via `http`. `credential` is the JSON `{key, region}` blob.
pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let cred: Credential =
        serde_json::from_str(credential).map_err(|e| VendorError::Parse(e.to_string()))?;
    super::validate_header_safe(&cred.key)?;
    let base = match cred.region.as_deref() {
        Some("bigmodel-cn") => BASE_CN,
        _ => BASE_GLOBAL,
    };
    let quota_body = http.get(&format!("{base}{QUOTA_PATH}"), &cred.key)?;
    let mut q = parse(&quota_body)?;

    // Try subscription API for plan name + subscription expiry.
    // Prepend "GLM Coding Plan " so "Lite" → "GLM Coding Plan Lite".
    let sub_url = format!("{base}{SUBSCRIPTION_PATH}");
    if let Ok(sub_body) = http.get(&sub_url, &cred.key) {
        let (plan, expiry) = extract_subscription(&quota_body, &sub_body);
        if let Some(plan) = plan {
            let prefix = "GLM Coding Plan ";
            if !plan.starts_with(prefix) && !plan.starts_with("GLM") {
                q.plan_label = Some(format!("{prefix}{plan}"));
            } else {
                q.plan_label = Some(plan);
            }
        }

        // Subscription expiry is the PLAN end date — distinct from each
        // window's rolling reset. Store it on `expires_at`, leaving per-window
        // `resets_at` intact (5h/周/MCP each reset on their own cadence).
        if let Some(expiry) = expiry {
            q.expires_at = Some(expiry);
        }
    }

    Ok(q)
}

/// Default fetch (real network).
pub async fn fetch(credential: &str) -> Result<Quota, VendorError> {
    let cred = credential.to_string();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &cred))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

struct UreqHttp;
impl Http for UreqHttp {
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError> {
        let resp = ureq::get(url)
            .set("Authorization", &format!("Bearer {bearer}"))
            .set("Accept", "application/json")
            .call()
            .map_err(|e| VendorError::Network(e.to_string()))?;
        resp.into_string()
            .map_err(|e| VendorError::Network(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_multi_window_collects_all() {
        // 5h short window (usage 1000/remaining 700 → 30% used) with a reset marker,
        // weekly long window (usage 1000/remaining 220 → 78%), MCP TIME_LIMIT (90%).
        let body = r#"{"data":{"limits":[
            {"type":"TOKENS_LIMIT","unit":5,"number":300,"usage":1000,"remaining":700,"nextResetTime":1893456000},
            {"type":"TOKENS_LIMIT","unit":6,"number":1,"usage":1000,"remaining":220},
            {"type":"TIME_LIMIT","unit":5,"number":1,"usage":100,"remaining":10}
        ]}}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.vendor, "glm");
        assert_eq!(q.windows.len(), 3);
        let labels: Vec<&str> = q.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["5h", "周", "MCP 月"]);
        // 5h used = (1000-700)/1000 = 30%
        let win5h = q.windows.iter().find(|w| w.label == "5h").unwrap();
        assert!((win5h.used_pct - 30.0).abs() < 1e-6);
        // reset marker parsed to ISO
        assert!(win5h.resets_at.is_some());
        assert!(win5h.resets_at.as_deref().unwrap().starts_with("2030"));
        // MCP used = (100-10)/100 = 90% → Danger is worst
        assert_eq!(q.status, QuotaStatus::Danger);
        assert!(q.balance.is_none());
    }

    #[test]
    fn parse_percentage_field() {
        let body = r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":42.0}]}}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert!((q.windows[0].used_pct - 42.0).abs() < 1e-6);
        assert_eq!(q.windows[0].label, "5h");
        // no reset marker → resets_at = None
        assert!(q.windows[0].resets_at.is_none());
    }

    #[test]
    fn parse_iso_reset_marker() {
        let body = r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":10.0,"next_reset_time":"2030-01-01T00:00:00Z"}]}}"#;
        let q = parse(body).unwrap();
        assert!(q.windows[0]
            .resets_at
            .as_deref()
            .unwrap()
            .starts_with("2030"));
    }

    #[test]
    fn parse_empty_errors() {
        assert!(matches!(
            parse(r#"{"data":{"limits":[]}}"#),
            Err(VendorError::Empty)
        ));
    }

    #[test]
    fn fetch_with_resolves_region() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, url: &str, _: &str) -> Result<String, VendorError> {
                // Return quota data for quota path, empty subscription for sub path.
                if url.contains(SUBSCRIPTION_PATH) {
                    return Ok(r#"{"data":[]}"#.into());
                }
                Ok(r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":10.0}]}}"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"key":"sk-x","region":"bigmodel-cn"}"#).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert!((q.windows[0].used_pct - 10.0).abs() < 1e-6);
        assert!(q.plan_label.is_none()); // empty subscription → no plan

        let q2 = fetch_with(&Mock, r#"{"key":"sk-x"}"#).unwrap();
        assert!(!q2.windows.is_empty());
        assert!(q2.plan_label.is_none());
    }

    #[test]
    fn fetch_with_plan_from_subscription() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, url: &str, _: &str) -> Result<String, VendorError> {
                if url.contains(SUBSCRIPTION_PATH) {
                    return Ok(r#"{"data":[{"product_name":"Lite"}]}"#.into());
                }
                Ok(r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":10.0}]}}"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"key":"sk-x"}"#).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("GLM Coding Plan Lite"));
    }

    #[test]
    fn extract_subscription_picks_max_next_renew_time() {
        // 3 consecutive VALID subscriptions → latest nextRenewTime wins.
        struct Mock;
        impl Http for Mock {
            fn get(&self, url: &str, _: &str) -> Result<String, VendorError> {
                if url.contains(SUBSCRIPTION_PATH) {
                    return Ok(r#"{"data":[
                        {"nextRenewTime":"2027-03-17","productName":"GLM Coding Lite","status":"VALID"},
                        {"nextRenewTime":"2027-04-17","productName":"GLM Coding Lite","status":"VALID"},
                        {"nextRenewTime":"2027-05-17","productName":"GLM Coding Lite","status":"VALID"}
                    ]}"#.into());
                }
                Ok(r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":10.0}]}}"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"key":"sk-x"}"#).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("GLM Coding Lite"));
        // Subscription expiry is on `expires_at` (not overwritten onto windows).
        assert!(q.expires_at.is_some());
        assert!(q.expires_at.as_deref().unwrap().starts_with("2027-05"));
        // Per-window resets_at is untouched by the subscription overlay.
        assert!(q.windows[0].resets_at.is_none());
    }

    #[test]
    fn extract_subscription_skips_invalid_status() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, url: &str, _: &str) -> Result<String, VendorError> {
                if url.contains(SUBSCRIPTION_PATH) {
                    return Ok(r#"{"data":[
                        {"nextRenewTime":"2028-01-01","productName":"Lite","status":"EXPIRED"},
                        {"nextRenewTime":"2027-05-17","productName":"Lite","status":"VALID"}
                    ]}"#
                    .into());
                }
                Ok(r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":10.0}]}}"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"key":"sk-x"}"#).unwrap();
        // EXPIRED (2028) skipped, only VALID (2027-05) used → on expires_at.
        assert!(q.expires_at.as_deref().unwrap().starts_with("2027-05"));
        assert!(q.windows[0].resets_at.is_none());
    }
}
