//! 百炼 Token Plan (阿里云 Bailian, personal edition) adapter.
//!
//! Cookie-based (same binding category as Qoder). There is no public quota
//! API — the adapter talks to the same console gateway the Token Plan page
//! uses (reverse-engineered from bailian.console.aliyun.com):
//!
//!   POST https://bailian-cs.console.aliyun.com/data/api.json
//!        ?action=BroadScopeAspnGateway&product=sfm_bailian&api=<api>
//!   body: params=<urlencoded json>&region=cn-beijing
//!   (every call's Data MUST carry `cornerstoneParam` — verified empirically:
//!   without it the gateway answers "Bad Request"; `sec_token` is NOT
//!   validated and is omitted. Expired sessions answer
//!   errorCode="BailianGateway.Login.NotLogined".)
//!
//!   api = zeldaHttp.apikeyMgr./tokenplan/personal/api/v2/subscription
//!         → { instanceCode, specCode, status, startTime, endTime,
//!             remainingDays, autoRenewFlag }
//!   api = zeldaHttp.apikeyMgr./tokenplan/personal/api/v2/usage
//!         → { per1WeekPercentage, per1WeekResetTime }
//!
//! `per1WeekPercentage` is the fraction of the current weekly cycle ALREADY
//! USED (0.0–1.0; the console renders it as "周期用量 x%"). The weekly window
//! resets at `per1WeekResetTime` (unix ms). Subscription `endTime` drives the
//! card's 到期 tag.

use serde::Deserialize;

use super::types::{epoch_to_iso, Quota, QuotaStatus, QuotaWindow};
use super::VendorError;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";
const GATEWAY: &str = "https://bailian-cs.console.aliyun.com/data/api.json";
const API_SUBSCRIPTION: &str = "zeldaHttp.apikeyMgr./tokenplan/personal/api/v2/subscription";
const API_USAGE: &str = "zeldaHttp.apikeyMgr./tokenplan/personal/api/v2/usage";
/// Commodity code of the personal Token Plan subscription.
const COMMODITY_CODE: &str = "sfm_tokenplansolo_public_cn";

/// Parse the `{ cookie }` credential blob.
#[derive(Debug, Deserialize)]
struct Credential {
    cookie: String,
}

/// HTTP client. Injected for unit tests.
pub trait Http {
    fn call(&self, api: &str, body: &str, cookie: &str) -> Result<String, VendorError>;
}

// ── response models ────────────────────────────────────────────────────────
// Envelope: { code, successResponse, data: { DataV2: { ret, data: { data:
// <payload>, success, code } } } } — triple-nested by the console gateway.

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    code: String,
    #[serde(default, rename = "successResponse")]
    success_response: bool,
    data: Option<EnvelopeData>,
}

#[derive(Debug, Deserialize)]
struct EnvelopeData {
    #[serde(rename = "DataV2")]
    data_v2: Option<DataV2>,
    /// Present on gateway-level failures instead of DataV2 (e.g. a request
    /// without `cornerstoneParam` → success=false, errorCode="Bad Request").
    #[serde(default)]
    success: Option<bool>,
    #[serde(default, rename = "errorCode")]
    error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DataV2 {
    #[serde(default)]
    #[allow(dead_code)] // present in payloads; kept for Debug fidelity
    ret: Option<Vec<String>>,
    data: Option<InnerData>,
}

#[derive(Debug, Deserialize)]
struct InnerData {
    success: Option<bool>,
    #[serde(default)]
    #[allow(dead_code)] // present in payloads; kept for Debug fidelity
    code: Option<String>,
    data: Option<serde_json::Value>,
}

/// Navigate to the innermost payload `Value`. Distinguishes failure shapes:
///   - outer successResponse=false / code≠200        → expired session (Auth)
///   - data.success=false + errorCode="Bad Request"   → malformed request (Parse)
///   - DataV2 present but inner success=false         → expired session (Auth)
fn unwrap_payload(body: &str) -> Result<serde_json::Value, VendorError> {
    let env: Envelope =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    if !env.success_response || env.code != "200" {
        // Gateway-level failure: typically an expired session.
        return Err(VendorError::Auth("Cookie 已过期，请重新获取".into()));
    }
    let data = env
        .data
        .ok_or_else(|| VendorError::Auth("Cookie 已过期，请重新获取".into()))?;
    let inner = match data.data_v2.and_then(|v2| v2.data) {
        Some(inner) => inner,
        None => {
            // No DataV2 → gateway refused the request itself.
            if data.success == Some(false) {
                if data.error_code.as_deref() == Some("Bad Request") {
                    return Err(VendorError::Parse("请求被网关拒绝 (Bad Request)".into()));
                }
                return Err(VendorError::Auth("Cookie 已过期，请重新获取".into()));
            }
            return Err(VendorError::Parse("响应缺少数据载荷".into()));
        }
    };
    if inner.success == Some(false) {
        return Err(VendorError::Auth("Cookie 已过期，请重新获取".into()));
    }
    inner.data.ok_or(VendorError::Empty)
}

#[derive(Debug, Clone, Default)]
struct Subscription {
    spec_code: String,
    status: String,
    /// Unix-ms subscription end time.
    end_time_ms: Option<i64>,
}

fn parse_subscription(payload: &serde_json::Value) -> Option<Subscription> {
    Some(Subscription {
        spec_code: payload
            .get("specCode")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        status: payload
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        end_time_ms: payload
            .get("endTime")
            .and_then(|v| v.as_i64())
            .filter(|&t| t > 0),
    })
}

#[derive(Debug, Clone, Default)]
struct Usage {
    /// Fraction of the weekly allowance already used (0.0–1.0).
    used_fraction: Option<f64>,
    /// Unix-ms time when the weekly window resets.
    reset_time_ms: Option<i64>,
}

fn parse_usage(payload: &serde_json::Value) -> Usage {
    Usage {
        used_fraction: payload
            .get("per1WeekPercentage")
            .and_then(|v| v.as_f64())
            .filter(|&f| (0.0..=1.0).contains(&f))
            .or_else(|| {
                // Some deployments may return an already-scaled percent.
                payload
                    .get("per1WeekPercentage")
                    .and_then(|v| v.as_f64())
                    .filter(|&f| f > 1.0 && f <= 100.0)
                    .map(|f| f / 100.0)
            }),
        reset_time_ms: payload
            .get("per1WeekResetTime")
            .and_then(|v| v.as_i64())
            .filter(|&t| t > 0),
    }
}

/// Human label for the plan spec ("lite" → "Lite").
fn spec_label(spec: &str) -> String {
    let s = spec.trim();
    if s.is_empty() {
        return "Token Plan".into();
    }
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "Token Plan".into(),
    }
}

/// Console routing context. Verified empirically: the gateway REQUIRES this
/// inside `Data` (an empty `Data` → "Bad Request"); `sec_token` is NOT
/// validated (a garbage value passes) and is omitted entirely.
const CORNERSTONE: &str = r#"{"protocol":"V2","console":"ONE_CONSOLE","productCode":"p_efm","domain":"bailian.console.aliyun.com","consoleSite":"BAILIAN_ALIYUN"}"#;

/// Build the request body (params JSON) for one gateway call. Every call
/// carries `cornerstoneParam` in `Data` (see CORNERSTONE note); `data_json`
/// holds ADDITIONAL key-value pairs merged as siblings inside `Data`
/// (e.g. `"queryInstanceInfoRequest":{...}`) — NOT a nested `{...}` object.
fn build_params(api: &str, data_json: &str) -> String {
    let extra = if data_json.is_empty() {
        String::new()
    } else {
        format!(",{data_json}")
    };
    let mut s = String::from(r#"{"Api":"#);
    s.push_str(&serde_json::json!(api).to_string());
    s.push_str(r#","V":"1.0","Data":{"cornerstoneParam":"#);
    s.push_str(CORNERSTONE);
    s.push_str(&extra);
    s.push('}');
    s.push('}');
    s
}

/// Fetch via `http`. `credential` is the JSON `{cookie}` blob.
pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let cred: Credential =
        serde_json::from_str(credential).map_err(|e| VendorError::Parse(e.to_string()))?;
    let cookie = cred.cookie.trim();
    if cookie.is_empty() {
        return Err(VendorError::Parse("缺少 Cookie".into()));
    }

    // 1. subscription — instance/spec/expiry.
    let sub_body = http.call(
        API_SUBSCRIPTION,
        &build_params(
            API_SUBSCRIPTION,
            &format!(r#""queryInstanceInfoRequest":{{"commodityCode":"{COMMODITY_CODE}"}}"#),
        ),
        cookie,
    )?;
    let sub_payload = unwrap_payload(&sub_body)?;
    let sub = parse_subscription(&sub_payload).ok_or(VendorError::Empty)?;
    if sub.status != "VALID" || sub.spec_code.is_empty() {
        // Authenticated but no active Token Plan subscription.
        return Err(VendorError::Empty);
    }

    // 2. usage — weekly cycle percentage + reset time (best-effort: a missing
    //    usage payload still shows the subscription window without a bar %).
    let usage = http
        .call(API_USAGE, &build_params(API_USAGE, ""), cookie)
        .ok()
        .and_then(|b| unwrap_payload(&b).ok())
        .map(|p| parse_usage(&p))
        .unwrap_or_default();

    // ── Build window ─────────────────────────────────────────────────
    let used_pct = usage
        .used_fraction
        .map(|f| (f * 100.0).clamp(0.0, 100.0))
        .unwrap_or(0.0);
    let window = QuotaWindow {
        label: "周".into(),
        used_pct,
        resets_at: usage.reset_time_ms.and_then(|ms| epoch_to_iso(ms as f64)),
        ..Default::default()
    };
    let status = QuotaStatus::from_used_pct(used_pct);

    Ok(Quota {
        vendor: "bailian".into(),
        plan_label: Some(format!("Token Plan · {}", spec_label(&sub.spec_code))),
        status,
        windows: vec![window],
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: sub.end_time_ms.and_then(|ms| epoch_to_iso(ms as f64)),
        site: None,
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
    fn call(&self, api: &str, body: &str, cookie: &str) -> Result<String, VendorError> {
        let resp = ureq::post(&format!(
            "{GATEWAY}?action=BroadScopeAspnGateway&product=sfm_bailian&api={api}"
        ))
        .set("Cookie", cookie)
        .set("Accept", "application/json, text/plain, */*")
        .set("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
        .set("User-Agent", USER_AGENT)
        .set("Origin", "https://bailian.console.aliyun.com")
        .set("Referer", "https://bailian.console.aliyun.com/")
        .send_form(&[("params", body), ("region", "cn-beijing")])
        .map_err(|e| VendorError::Network(e.to_string()))?;
        resp.into_string()
            .map_err(|e| VendorError::Network(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mock {
        sub: String,
        usage: String,
    }
    impl Http for Mock {
        fn call(&self, api: &str, _body: &str, _cookie: &str) -> Result<String, VendorError> {
            if api.contains("subscription") {
                Ok(self.sub.clone())
            } else if api.contains("usage") {
                Ok(self.usage.clone())
            } else {
                Err(VendorError::Parse("unexpected api".into()))
            }
        }
    }

    fn envelope(inner: &str) -> String {
        let mut s = String::from(
            r#"{"code":"200","successResponse":true,"data":{"DataV2":{"ret":["SUCCESS"],"data":{"success":true,"code":"SUCCESS","data":"#,
        );
        s.push_str(inner);
        s.push_str(r#"}}}}"#);
        s
    }

    const SUB_OK: &str = r#"{"instanceCode":"sfm_x","specCode":"lite","remainingDays":91,"startTime":1787729847000,"endTime":1795708800000,"autoRenewFlag":false,"status":"VALID"}"#;
    const USAGE_OK: &str =
        r#"{"per1WeekPercentage":0.099488872,"per1WeekResetTime":1788335340000}"#;

    const COOKIE: &str = r#"{"cookie":"XSRF-TOKEN=abc; login_aliyunid=xyz"}"#;

    #[test]
    fn fetch_with_builds_weekly_window() {
        let m = Mock {
            sub: envelope(SUB_OK),
            usage: envelope(USAGE_OK),
        };
        let q = fetch_with(&m, COOKIE).unwrap();
        assert_eq!(q.vendor, "bailian");
        assert_eq!(q.plan_label.as_deref(), Some("Token Plan · Lite"));
        assert_eq!(q.windows.len(), 1);
        let w = &q.windows[0];
        assert_eq!(w.label, "周");
        assert!((w.used_pct - 9.9488872).abs() < 0.001);
        assert!(w.resets_at.as_deref().unwrap().starts_with("2026-"));
        assert!(q.expires_at.as_deref().unwrap().starts_with("2026-"));
        assert_eq!(q.status, QuotaStatus::Ok);
    }

    #[test]
    fn fetch_with_expired_cookie_is_auth_error() {
        let m = Mock {
            sub: r#"{"code":"200","successResponse":false,"data":{}}"#.into(),
            usage: envelope(USAGE_OK),
        };
        match fetch_with(&m, COOKIE) {
            Err(VendorError::Auth(msg)) => assert!(msg.contains("Cookie")),
            other => panic!("expected Auth, got {other:?}"),
        }
    }

    #[test]
    fn fetch_with_inner_failure_is_auth_error() {
        // Gateway 200 but DataV2.inner.success=false (session dropped).
        let m = Mock {
            sub: r#"{"code":"200","successResponse":true,"data":{"DataV2":{"ret":["FAIL"],"data":{"success":false,"code":"FAIL","data":null}}}}"#.into(),
            usage: envelope(USAGE_OK),
        };
        assert!(matches!(fetch_with(&m, COOKIE), Err(VendorError::Auth(_))));
    }

    #[test]
    fn fetch_with_no_active_subscription_is_empty() {
        let m = Mock {
            sub: envelope(r#"{"instanceCode":"i","specCode":"lite","status":"EXPIRED"}"#),
            usage: envelope(USAGE_OK),
        };
        assert!(matches!(fetch_with(&m, COOKIE), Err(VendorError::Empty)));
    }

    #[test]
    fn fetch_with_requires_cookie() {
        let m = Mock {
            sub: envelope(SUB_OK),
            usage: envelope(USAGE_OK),
        };
        assert!(matches!(
            fetch_with(&m, r#"{"cookie":"  "}"#),
            Err(VendorError::Parse(_))
        ));
    }

    #[test]
    fn fetch_with_tolerates_usage_failure() {
        // Subscription OK but usage endpoint breaks → window with 0%.
        let m = Mock {
            sub: envelope(SUB_OK),
            usage: r#"{"code":"500"}"#.into(),
        };
        let q = fetch_with(&m, COOKIE).unwrap();
        assert_eq!(q.windows[0].used_pct, 0.0);
        assert!(q.windows[0].resets_at.is_none());
    }

    #[test]
    fn percentage_above_one_treated_as_percent() {
        // Some deployments return 9.95 instead of 0.0995.
        let m = Mock {
            sub: envelope(SUB_OK),
            usage: envelope(r#"{"per1WeekPercentage":9.95,"per1WeekResetTime":1788335340000}"#),
        };
        let q = fetch_with(&m, COOKIE).unwrap();
        assert!((q.windows[0].used_pct - 9.95).abs() < 0.01);
    }

    #[test]
    fn usage_close_to_exhaustion_is_danger() {
        let m = Mock {
            sub: envelope(SUB_OK),
            usage: envelope(r#"{"per1WeekPercentage":0.98,"per1WeekResetTime":1788335340000}"#),
        };
        let q = fetch_with(&m, COOKIE).unwrap();
        assert_eq!(q.status, QuotaStatus::Danger);
    }

    #[test]
    fn bad_request_shape_is_parse_error_not_auth() {
        // Gateway refusal without DataV2 (e.g. missing cornerstoneParam) must
        // NOT be reported as an expired cookie.
        let m = Mock {
            sub: r#"{"code":"200","successResponse":true,"data":{"success":false,"httpStatus":200,"errorCode":"Bad Request","errorMsg":"Bad Request"}}"#.into(),
            usage: envelope(USAGE_OK),
        };
        match fetch_with(&m, COOKIE) {
            Err(VendorError::Parse(msg)) => assert!(msg.contains("Bad Request")),
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    #[test]
    fn not_logined_shape_is_auth_error() {
        // Real expired-session shape observed from the gateway: outer
        // successResponse=true, data.success=false, no DataV2,
        // errorCode="BailianGateway.Login.NotLogined" → Auth.
        let m = Mock {
            sub: r#"{"code":"200","successResponse":true,"data":{"success":false,"httpStatus":200,"errorCode":"BailianGateway.Login.NotLogined","errorMsg":"not logined"}}"#.into(),
            usage: envelope(USAGE_OK),
        };
        assert!(matches!(fetch_with(&m, COOKIE), Err(VendorError::Auth(_))));
    }

    #[test]
    fn build_params_escapes_api_name_and_injects_cornerstone() {
        // Empty data → no trailing comma.
        let p = build_params(API_USAGE, "");
        let v: serde_json::Value = serde_json::from_str(&p).unwrap();
        assert_eq!(v["Api"], API_USAGE);
        assert_eq!(v["Data"]["cornerstoneParam"]["console"], "ONE_CONSOLE");
        // Non-empty data → merged after cornerstoneParam.
        let p2 = build_params(API_SUBSCRIPTION, r#""x":1"#);
        let v2: serde_json::Value = serde_json::from_str(&p2).unwrap();
        assert_eq!(v2["Data"]["x"], 1);
        assert!(v2["Data"]["cornerstoneParam"].is_object());
    }
}
