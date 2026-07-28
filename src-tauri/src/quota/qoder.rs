//! Qoder (by Alibaba) big-model credits adapter.
//!
//! Cookie-based, with a cn/global site split. Credential is
//! `{ cookie, site }` where `site` ∈ {"cn","global"} (Account.svelte select).
//!   GET {origin}/api/v2/me/usages/big_model_credits  → totalQuota + sharedQuota
//!   GET {origin}/api/v1/me/userplan                  → plan label (optional)
//! Both endpoints are plain JSON. The usage response carries `totalQuota` and
//! an optional `sharedQuota`; we merge them into a single `billing` window
//! labeled "Credits".
//!
//! Faithfully ported from token-monitor src/shared/qoderLimits.js.

use serde::Deserialize;

use super::types::{epoch_to_iso, parse_iso, Quota, QuotaStatus, QuotaWindow, QuotaWindowSubItem};
use super::VendorError;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

/// Parse the `{ cookie, site }` credential blob.
#[derive(Debug, Deserialize)]
struct Credential {
    cookie: String,
    #[serde(default)]
    site: Option<String>,
}

/// HTTP client. Injected for unit tests.
pub trait Http {
    fn get(&self, url: &str, cookie: &str, origin: &str) -> Result<String, VendorError>;
}

fn origin_for(site: &str) -> &'static str {
    match site {
        "cn" => "https://qoder.com.cn",
        _ => "https://qoder.com",
    }
}

fn read<'a>(obj: &'a serde_json::Value, camel: &str, snake: &str) -> Option<&'a serde_json::Value> {
    obj.get(camel).or_else(|| obj.get(snake))
}

fn number_from_value(v: Option<&serde_json::Value>) -> Option<f64> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Per-bucket parsed data from `quota_summary`.
fn reset_marker(v: Option<&serde_json::Value>) -> Option<String> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().and_then(super::types::epoch_to_iso),
        Some(serde_json::Value::String(s)) => parse_iso(s),
        _ => None,
    }
}

/// Per-bucket parsed data from `quota_summary`.
#[derive(Debug, Clone, Default)]
struct Bucket {
    used: f64,
    total: f64,
    /// Direct from API's `usage_percentage` field (authoritative).
    usage_pct: f64,
}

impl Bucket {
    fn from_summary(container: &serde_json::Value) -> Option<Self> {
        let summary = read(container, "quotaSummary", "quota_summary")?;
        let used = number_from_value(read(summary, "usedValue", "used_value"))?;
        let total = number_from_value(read(summary, "limitValue", "limit_value"))?;
        if used < 0.0 || total < 0.0 {
            return None;
        }
        let api_pct = number_from_value(read(summary, "usagePercentage", "usage_percentage"));
        let usage_pct = api_pct.unwrap_or_else(|| {
            if total > 0.0 { (used / total * 100.0).clamp(0.0, 100.0) } else { 0.0 }
        });
        Some(Self { used, total, usage_pct: usage_pct.clamp(0.0, 100.0) })
    }

    /// Extract individual items from `quota_detail[]`, sorted ascending by
    /// expiry (earliest first; items without expiry last). Returns `None` when
    /// the array is absent or empty.
    fn sub_items(container: &serde_json::Value) -> Option<Vec<QuotaWindowSubItem>> {
        let arr = container.get("quota_detail")?.as_array()?;
        let mut items = Vec::new();
        for detail in arr {
            let used = number_from_value(detail.get("used_value")).unwrap_or(0.0);
            let total = number_from_value(detail.get("limit_value")).unwrap_or(0.0);
            if total <= 0.0 && used <= 0.0 {
                continue; // skip empty entries
            }
            let api_pct = number_from_value(detail.get("usage_percentage"));
            let pct = api_pct.unwrap_or_else(|| {
                if total > 0.0 { (used / total * 100.0).clamp(0.0, 100.0) } else { 0.0 }
            });
            let name = detail.get("source")
                .and_then(|v| v.as_str())
                .map(|s| {
                    match s {
                        "PLAN" => "订阅".into(),
                        "RESOURCE_PACKAGE_SOURCE_BONUS" => "资源包".into(),
                        other => other.into(),
                    }
                })
                .unwrap_or_default();
            let expires_at = detail.get("expires_at")
                .and_then(|v| v.as_i64())
                .filter(|&ms| ms > 0)
                .and_then(|ms| epoch_to_iso(ms as f64));
            items.push(QuotaWindowSubItem { name, used, total, pct, expires_at });
        }
        // Ascending by expiry (earliest first); items without expiry sink to end.
        items.sort_by(|a, b| match (&a.expires_at, &b.expires_at) {
            (Some(ae), Some(be)) => ae.cmp(be),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        if items.is_empty() { None } else { Some(items) }
    }
}

/// Detailed parse result used by `fetch_with` to build separate windows.
#[derive(Debug, Clone, Default)]
struct DetailedParsed {
    plan: Bucket,
    pkg: Bucket,
    total_pct: f64,
    top_next_reset: Option<String>,
    /// Whether `plan_quota` / `resource_package_quota` keys were present.
    has_plan: bool,
    has_pkg: bool,
    /// Individual items from `plan_quota.quota_detail[]` (plan has 1 item).
    plan_items: Vec<QuotaWindowSubItem>,
    /// Individual items from `resource_package_quota.quota_detail[]` (many).
    pkg_items: Vec<QuotaWindowSubItem>,
}

/// Parse the usage payload into per-bucket breakdown.
fn parse_detailed(body: &str) -> Result<DetailedParsed, VendorError> {
    let root: serde_json::Value =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    let payload = root
        .get("data")
        .filter(|d| d.is_object())
        .unwrap_or(&root);

    let total_guard = read(payload, "totalQuota", "total_quota")
        .and_then(Bucket::from_summary)
        .ok_or(VendorError::Parse(
            "missing total_quota.quota_summary".into(),
        ))?;

    let has_plan = read(payload, "planQuota", "plan_quota").is_some();
    let has_pkg = read(payload, "resourcePackageQuota", "resource_package_quota").is_some();

    let plan = read(payload, "planQuota", "plan_quota")
        .and_then(Bucket::from_summary)
        .unwrap_or_default();
    let pkg = read(payload, "resourcePackageQuota", "resource_package_quota")
        .and_then(Bucket::from_summary)
        .unwrap_or_default();

    let plan_items: Vec<QuotaWindowSubItem> = read(payload, "planQuota", "plan_quota")
        .and_then(Bucket::sub_items)
        .unwrap_or_default();
    let pkg_items: Vec<QuotaWindowSubItem> = read(payload, "resourcePackageQuota", "resource_package_quota")
        .and_then(Bucket::sub_items)
        .unwrap_or_default();

    let top_next_reset = reset_marker(read(payload, "nextResetAt", "next_reset_at"));

    Ok(DetailedParsed {
        plan,
        pkg,
        total_pct: total_guard.usage_pct,
        top_next_reset,
        has_plan,
        has_pkg,
        plan_items,
        pkg_items,
    })
}

/// Legacy wrapper: total usage percentage + single reset time.
pub fn parse(body: &str) -> Result<(f64, Option<String>), VendorError> {
    let d = parse_detailed(body)?;
    // When plan/pkg buckets exist (limit > 0), compute from their sum; otherwise
    // fall back to the total bucket's own percentage.
    if d.plan.total > 0.0 || d.pkg.total > 0.0 {
        let u = d.plan.used + d.pkg.used;
        let t = d.plan.total + d.pkg.total;
        Ok(((u / t * 100.0).clamp(0.0, 100.0), d.top_next_reset))
    } else {
        Ok((d.total_pct, d.top_next_reset))
    }
}

/// Normalize a plan-tier enum into a friendly label (token-monitor planText).
fn plan_text(raw: &str) -> String {
    let normalized = raw
        .trim_start_matches("ORGANIZATION_PLAN_TIER_")
        .trim_start_matches("PLAN_TIER_")
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let known = [
        ("free", "Community Edition"),
        ("community", "Community Edition"),
        ("communityedition", "Community Edition"),
        ("community edition", "Community Edition"),
        ("protrial", "Pro Trial"),
        ("pro trial", "Pro Trial"),
        ("pro", "Pro"),
        ("proplus", "Pro+"),
        ("pro plus", "Pro+"),
        ("pro+", "Pro+"),
        ("ultra", "Ultra"),
        ("team", "Teams"),
        ("teams", "Teams"),
        ("enterprise", "Enterprise"),
    ];
    for (k, v) in known {
        if normalized == k {
            return v.into();
        }
    }
    // Fallback: title-case words, fold "pro plus" → "Pro+".
    raw.replace(['_', '-'], " ")
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .replace("Pro Plus", "Pro+")
}

/// Extract a plan label from the userplan response (token-monitor firstPlanLabel).
fn parse_plan_label(body: &str) -> Option<String> {
    let root: serde_json::Value = serde_json::from_str(body).ok()?;
    let candidates: Vec<&serde_json::Value> = std::iter::once(&root)
        .chain(root.get("data"))
        .collect();
    for source in candidates {
        for field in [
            "plan_tier", "planTier", "plan", "tier", "name", "product_name",
            "productName", "subscription_type", "subscriptionType",
        ] {
            if let Some(s) = source.get(field).and_then(|v| v.as_str()) {
                let label = plan_text(s);
                if !label.is_empty() {
                    return Some(label);
                }
            }
        }
    }
    None
}

/// Fetch via `http`. `credential` is the JSON `{cookie, site}` blob.
pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let cred: Credential =
        serde_json::from_str(credential).map_err(|e| VendorError::Parse(e.to_string()))?;
    let cookie = cred.cookie.trim();
    if cookie.is_empty() {
        return Err(VendorError::Parse("缺少 Cookie".into()));
    }
    let site = cred.site.as_deref().unwrap_or("global");
    let origin = origin_for(site);

    let usage_url = format!("{origin}/api/v2/me/usages/big_model_credits");
    let body = http.get(&usage_url, cookie, origin)?;

    let parsed = parse_detailed(&body)?;

    // Fetch userplan for plan label only (expiry is per-item, not header-level).
    let plan_label = {
        let plan_url = format!("{origin}/api/v1/me/userplan");
        match http.get(&plan_url, cookie, origin) {
            Ok(plan_body) => parse_plan_label(&plan_body),
            Err(_) => None,
        }
    };

    // ── Build windows ────────────────────────────────────────────────
    // Both 订阅 and 资源包 follow the same shape: a summary window whose
    // `used_value`/`total_value` render as a caption under the bar, plus
    // expandable `sub_items` (each with its own expiry caption). A bucket is
    // shown when it carries meaningful data (limit > 0 or any sub-item).
    let mut windows: Vec<QuotaWindow> = Vec::new();

    let plan_meaningful = parsed.has_plan
        && (parsed.plan.total > 0.0 || !parsed.plan_items.is_empty());
    if plan_meaningful {
        windows.push(QuotaWindow {
            label: "订阅".into(),
            used_pct: parsed.plan.usage_pct,
            resets_at: None,
            used_value: Some(parsed.plan.used),
            total_value: Some(parsed.plan.total),
            sub_items: if parsed.plan_items.is_empty() { None } else { Some(parsed.plan_items) },
        });
    }

    let pkg_meaningful = parsed.has_pkg
        && (parsed.pkg.total > 0.0 || !parsed.pkg_items.is_empty());
    if pkg_meaningful {
        windows.push(QuotaWindow {
            label: "资源包".into(),
            used_pct: parsed.pkg.usage_pct,
            resets_at: None,
            used_value: Some(parsed.pkg.used),
            total_value: Some(parsed.pkg.total),
            sub_items: if parsed.pkg_items.is_empty() { None } else { Some(parsed.pkg_items) },
        });
    }

    // Neither bucket meaningful → fall back to totalQuota as a single "Credits" window.
    if windows.is_empty() {
        windows.push(QuotaWindow {
            label: "Credits".into(),
            used_pct: parsed.total_pct,
            resets_at: parsed.top_next_reset,
            ..Default::default()
        });
    }

    // Overall status = worst of all windows.
    let status = QuotaStatus::worst_of(windows.iter().map(|w| QuotaStatus::from_used_pct(w.used_pct)));

    Ok(Quota {
        vendor: "qoder".into(),
        plan_label,
        status,
        windows,
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        // No header-level expiry: Qoder expiry is per resource-package / sub-item.
        expires_at: None,
    })
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
    fn get(&self, url: &str, cookie: &str, origin: &str) -> Result<String, VendorError> {
        let resp = ureq::get(url)
            .set("Cookie", cookie)
            .set("Accept", "application/json, text/plain, */*")
            .set("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .set("User-Agent", USER_AGENT)
            .set("Origin", origin)
            .set("Referer", &format!("{origin}/account/usage"))
            .set("X-Requested-With", "XMLHttpRequest")
            // Qoder echoes CSRF via the sec-fetch-site header; this literal token
            // value is what the web client sends (see real request capture).
            .set("X-Csrf-Token", "_echo_csrf_using_sec_fetch_site_")
            .set("Sec-Fetch-Dest", "empty")
            .set("Sec-Fetch-Mode", "cors")
            .set("Sec-Fetch-Site", "same-origin")
            .set("Bx-V", "2.5.35")
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
    fn parse_detailed_real_response_uses_api_percentages() {
        // Real Qoder shape: plan=0/0 (free), pkg=300/1500=20%.
        let body = r#"{
            "quota_key":"big_model_credits",
            "status":"active",
            "plan_quota":{"quota_summary":{"used_value":0,"limit_value":0,"remaining_value":0,"usage_percentage":0,"unit":"credits"}},
            "resource_package_quota":{"quota_summary":{"used_value":300,"limit_value":1500,"remaining_value":1200,"usage_percentage":20,"unit":"credits"}},
            "total_quota":{"quota_summary":{"used_value":300,"limit_value":1500,"remaining_value":1200,"usage_percentage":20,"unit":"credits"}},
            "nextResetAt":1785037446378
        }"#;
        let d = parse_detailed(body).unwrap();
        // plan: 0/0 → free plan
        assert_eq!(d.plan.total, 0.0);
        // pkg: uses API's usage_percentage=20, not computed 20
        assert!((d.pkg.usage_pct - 20.0).abs() < 1e-6);
        assert_eq!(d.pkg.used, 300.0);
        assert_eq!(d.pkg.total, 1500.0);
        assert!(d.top_next_reset.is_some());
    }

    #[test]
    fn parse_detailed_zero_usage() {
        let body = r#"{
            "plan_quota":{"quota_summary":{"used_value":0,"limit_value":0,"remaining_value":0,"usage_percentage":0,"unit":"credits"}},
            "resource_package_quota":{"quota_summary":{"used_value":0,"limit_value":1500,"remaining_value":1500,"usage_percentage":0,"unit":"credits"}},
            "total_quota":{"quota_summary":{"used_value":0,"limit_value":1500,"remaining_value":1500,"usage_percentage":0,"unit":"credits"}},
            "nextResetAt":1785037446378
        }"#;
        let d = parse_detailed(body).unwrap();
        assert_eq!(d.plan.total, 0.0);
        assert_eq!(d.pkg.used, 0.0);
        assert_eq!(d.pkg.total, 1500.0);
        assert!((d.pkg.usage_pct - 0.0).abs() < 1e-6);
        assert!(d.top_next_reset.is_some());
    }

    #[test]
    fn parse_detailed_pkg_items_sorted_ascending_by_expiry() {
        // Multiple resource packages with different expires_at; the parser
        // must return them sorted ascending (earliest first).
        let body = r#"{
            "total_quota":{"quota_summary":{"used_value":0,"limit_value":300,"remaining_value":300,"usage_percentage":0,"unit":"credits"}},
            "resource_package_quota":{
                "quota_summary":{"used_value":0,"limit_value":300,"remaining_value":300,"usage_percentage":0,"unit":"credits"},
                "quota_detail":[
                    {"used_value":0,"limit_value":100,"expires_at":1787792416209,"source":"RESOURCE_PACKAGE_SOURCE_BONUS"},
                    {"used_value":0,"limit_value":100,"expires_at":1785037446000,"source":"RESOURCE_PACKAGE_SOURCE_BONUS"},
                    {"used_value":0,"limit_value":100,"expires_at":1786952586166,"source":"RESOURCE_PACKAGE_SOURCE_BONUS"}
                ]
            },
            "nextResetAt":1785037446378
        }"#;
        let d = parse_detailed(body).unwrap();
        assert_eq!(d.pkg_items.len(), 3);
        // Ascending: 1785037446000 < 1786952586166 < 1787792416209
        let exps: Vec<&str> = d.pkg_items.iter().map(|i| i.expires_at.as_deref().unwrap_or("")).collect();
        assert!(exps[0] < exps[1]);
        assert!(exps[1] < exps[2]);
    }

    #[test]
    fn parse_legacy_total_and_shared_merged() {
        let body = r#"{"data":{
            "totalQuota":{"quotaSummary":{"usedValue":30,"limitValue":100,"remainingValue":70,"usagePercentage":30}},
            "nextResetAt":"2030-01-01T00:00:00Z"
        }}"#;
        let (pct, resets) = parse(body).unwrap();
        assert!((pct - 30.0).abs() < 1e-3);
        assert!(resets.as_deref().unwrap().starts_with("2030"));
    }

    #[test]
    fn parse_missing_total_errors() {
        let body = r#"{"data":{"sharedQuota":{"quotaSummary":{"usedValue":10,"limitValue":50}}}}"#;
        assert!(parse(body).is_err());
    }

    #[test]
    fn parse_snake_case_fields() {
        let body = r#"{"data":{"total_quota":{"quota_summary":{"used_value":50,"limit_value":100,"usage_percentage":50}}}}"#;
        let (pct, _) = parse(body).unwrap();
        assert!((pct - 50.0).abs() < 1e-6);
    }

    #[test]
    fn plan_text_normalizes_tiers() {
        assert_eq!(plan_text("PLAN_TIER_PRO_PLUS"), "Pro+");
        assert_eq!(plan_text("PLAN_TIER_FREE"), "Community Edition");
        assert_eq!(plan_text("PLAN_TIER_TEAMS"), "Teams");
    }

    #[test]
    fn fetch_with_returns_separate_windows_for_plan_and_pkg() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, url: &str, _cookie: &str, _origin: &str) -> Result<String, VendorError> {
                if url.contains("/userplan") {
                    return Ok(r#"{"data":{"plan_tier":"PLAN_TIER_PRO","end_date":1900000000000}}"#.into());
                }
                Ok(r#"{
                    "plan_quota":{"quota_summary":{"used_value":20,"limit_value":100,"remaining_value":80,"usage_percentage":20,"unit":"credits"},"quota_detail":[{"used_value":20,"limit_value":100,"usage_percentage":20,"source":"PLAN","expires_at":1900000000000}]},
                    "resource_package_quota":{"quota_summary":{"used_value":50,"limit_value":500,"remaining_value":450,"usage_percentage":10,"unit":"credits"},"quota_detail":[{"used_value":30,"limit_value":300,"usage_percentage":10,"source":"RESOURCE_PACKAGE_SOURCE_BONUS","expires_at":1787792416209},{"used_value":20,"limit_value":200,"usage_percentage":10,"source":"RESOURCE_PACKAGE_SOURCE_BONUS","expires_at":1786952586166}]},
                    "total_quota":{"quota_summary":{"used_value":70,"limit_value":600,"remaining_value":530,"usage_percentage":11.7,"unit":"credits"}},
                    "nextResetAt":1785037446378
                }"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"cookie":"c=1","site":"global"}"#).unwrap();
        assert_eq!(q.vendor, "qoder");
        assert_eq!(q.windows.len(), 2);
        // Window 0: 订阅
        assert_eq!(q.windows[0].label, "订阅");
        assert!((q.windows[0].used_pct - 20.0).abs() < 1e-6);
        assert_eq!(q.windows[0].used_value, Some(20.0));
        assert_eq!(q.windows[0].total_value, Some(100.0));
        assert!(q.windows[0].sub_items.is_some());
        let plan_items = q.windows[0].sub_items.as_ref().unwrap();
        assert_eq!(plan_items.len(), 1);
        assert_eq!(plan_items[0].name, "订阅");
        assert_eq!(plan_items[0].used, 20.0);
        assert_eq!(plan_items[0].total, 100.0);
        assert!(plan_items[0].expires_at.is_some());
        // Window 1: 资源包
        assert_eq!(q.windows[1].label, "资源包");
        assert!((q.windows[1].used_pct - 10.0).abs() < 1e-6);
        assert_eq!(q.windows[1].used_value, Some(50.0));
        assert_eq!(q.windows[1].total_value, Some(500.0));
        // With sub_items present, window-level resets_at is None (each
        // sub-item carries its own expiry).
        assert!(q.windows[1].sub_items.is_some());
        assert!(q.windows[1].resets_at.is_none());
        assert_eq!(q.plan_label.as_deref(), Some("Pro"));
        // No header-level expiry for Qoder (expiry is per sub-item).
        assert!(q.expires_at.is_none());
    }

    #[test]
    fn fetch_with_skips_empty_buckets() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, url: &str, _cookie: &str, _origin: &str) -> Result<String, VendorError> {
                if url.contains("/userplan") {
                    return Ok(r#"{"data":{"plan_tier":"PLAN_TIER_FREE"}}"#.into());
                }
                // plan=0/0 (free), pkg=0/1500 (resource only)
                Ok(r#"{
                    "plan_quota":{"quota_summary":{"used_value":0,"limit_value":0,"remaining_value":0,"usage_percentage":0,"unit":"credits"}},
                    "resource_package_quota":{"quota_summary":{"used_value":0,"limit_value":1500,"remaining_value":1500,"usage_percentage":0,"unit":"credits"}},
                    "total_quota":{"quota_summary":{"used_value":0,"limit_value":1500,"remaining_value":1500,"usage_percentage":0,"unit":"credits"}},
                    "nextResetAt":1785037446378
                }"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"cookie":"c=1","site":"global"}"#).unwrap();
        // Only 资源包 window (plan limit=0 → skipped)
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "资源包");
        assert_eq!(q.plan_label.as_deref(), Some("Community Edition"));
    }

    #[test]
    fn fetch_with_requires_cookie() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str, _: &str) -> Result<String, VendorError> {
                unreachable!()
            }
        }
        let err = fetch_with(&Mock, r#"{"cookie":"","site":"cn"}"#).unwrap_err();
        assert!(matches!(err, VendorError::Parse(_)));
    }

    #[test]
    fn fetch_with_resolves_cn_origin() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, url: &str, _: &str, _: &str) -> Result<String, VendorError> {
                assert!(url.starts_with("https://qoder.com.cn/"));
                Ok(r#"{
                    "plan_quota":{"quota_summary":{"used_value":1,"limit_value":100,"remaining_value":99,"usage_percentage":1,"unit":"credits"}},
                    "total_quota":{"quota_summary":{"used_value":1,"limit_value":100,"remaining_value":99,"usage_percentage":1,"unit":"credits"}},
                    "nextResetAt":1785037446378
                }"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"cookie":"c=1","site":"cn"}"#).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "订阅");
        assert!((q.windows[0].used_pct - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fetch_with_only_total_no_plan_or_pkg() {
        // Fallback: only total_quota, no plan/pkg breakdown.
        struct Mock;
        impl Http for Mock {
            fn get(&self, url: &str, _cookie: &str, _origin: &str) -> Result<String, VendorError> {
                if url.contains("/userplan") {
                    return Ok(r#"{"data":{"plan_tier":"PLAN_TIER_PRO"}}"#.into());
                }
                Ok(r#"{"totalQuota":{"quotaSummary":{"usedValue":80,"limitValue":100,"usagePercentage":80}}}"#.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"cookie":"c=1","site":"global"}"#).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "Credits");
        assert!((q.windows[0].used_pct - 80.0).abs() < 1e-6);
    }
}
