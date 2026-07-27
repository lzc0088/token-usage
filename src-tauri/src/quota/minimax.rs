//! MiniMax Token Plan quota adapter.
//!
//! `GET https://api.minimaxi.com/v1/token_plan/remains` (CN) or `.io` (global)
//! with `Authorization: Bearer <coding key>`. The response nests
//! `data.model_remains[]`; the `model_name === "general"` row is the coding
//! plan (video/voice rows are ignored). It carries two windows:
//!   - 5h session: `current_interval_remaining_percent` + `end_time`
//!   - weekly:     `current_weekly_remaining_percent` + `weekly_end_time`
//!
//! A `*_status` of 3 is a "no entitlement" placeholder lane and is suppressed.
//!
//! Faithfully ported from token-monitor src/shared/minimaxLimits.js.

use serde::Deserialize;

use super::types::{epoch_to_iso, Quota, QuotaStatus, QuotaWindow};
use super::VendorError;

const URL_CN: &str = "https://api.minimaxi.com/v1/token_plan/remains";

#[derive(Debug, Deserialize)]
struct Resp {
    #[serde(default)]
    data: RespData,
    /// Some responses put the array at the top level.
    #[serde(default)]
    model_remains: Vec<Bucket>,
}
#[derive(Debug, Default, Deserialize)]
struct RespData {
    #[serde(default)]
    model_remains: Vec<Bucket>,
}

#[derive(Debug, Deserialize)]
struct Bucket {
    #[serde(default)]
    model_name: String,
    #[serde(default)]
    current_interval_remaining_percent: Option<f64>,
    #[serde(default)]
    current_interval_status: Option<f64>,
    #[serde(default)]
    end_time: Option<f64>,
    #[serde(default)]
    current_weekly_remaining_percent: Option<f64>,
    #[serde(default)]
    current_weekly_status: Option<f64>,
    #[serde(default)]
    weekly_end_time: Option<f64>,
}

pub trait Http {
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError>;
}

/// A status==3 lane with null / ≥100 percent is the server's placeholder.
fn is_placeholder(pct: Option<f64>, status: Option<f64>) -> bool {
    if status.map(|s| s as i64) != Some(3) {
        return false;
    }
    match pct {
        None => true,
        Some(p) => p >= 100.0,
    }
}

/// Build a window from a remaining-percent + reset-epoch pair. `None` when the
/// lane is a placeholder or lacks a usable percent.
fn window(
    label: &str,
    remaining_pct: Option<f64>,
    status: Option<f64>,
    end_time: Option<f64>,
) -> Option<QuotaWindow> {
    if is_placeholder(remaining_pct, status) {
        return None;
    }
    let remain = remaining_pct?;
    let used = (100.0 - remain).clamp(0.0, 100.0);
    Some(QuotaWindow {
        label: label.into(),
        used_pct: used,
        resets_at: end_time.and_then(epoch_to_iso),
    })
}

/// Select the `general` bucket and build its 5h + weekly windows.
pub fn parse(body: &str) -> Result<Quota, VendorError> {
    let resp: Resp = serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    let rows = if !resp.data.model_remains.is_empty() {
        &resp.data.model_remains
    } else {
        &resp.model_remains
    };
    let bucket = rows
        .iter()
        .find(|b| b.model_name.eq_ignore_ascii_case("general"))
        .ok_or(VendorError::Empty)?;

    let mut windows = Vec::new();
    if let Some(w) = window(
        "5h",
        bucket.current_interval_remaining_percent,
        bucket.current_interval_status,
        bucket.end_time,
    ) {
        windows.push(w);
    }
    if let Some(w) = window(
        "周",
        bucket.current_weekly_remaining_percent,
        bucket.current_weekly_status,
        bucket.weekly_end_time,
    ) {
        windows.push(w);
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
        vendor: "minimax".into(),
        plan_label: Some("Token Plan".into()),
        status,
        windows,
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
    })
}

pub fn fetch_with(http: &dyn Http, api_key: &str) -> Result<Quota, VendorError> {
    // Credential may be JSON `{"key":"..."}` or a plain key string.
    let key = super::extract_key(api_key);
    super::validate_header_safe(&key)?;
    let body = http.get(URL_CN, &key)?;
    parse(&body)
}

pub async fn fetch(api_key: &str) -> Result<Quota, VendorError> {
    let key = api_key.to_string();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &key))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

struct UreqHttp;
impl Http for UreqHttp {
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError> {
        let resp = ureq::get(url)
            .set("Authorization", &format!("Bearer {bearer}"))
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
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
    fn parse_general_two_windows() {
        // general: 5h remaining 40% (→used 60), weekly remaining 15% (→used 85).
        let body = r#"{"data":{"model_remains":[
            {"model_name":"video","current_interval_remaining_percent":90.0},
            {"model_name":"general",
             "current_interval_remaining_percent":40.0,"end_time":1893456000,
             "current_weekly_remaining_percent":15.0,"weekly_end_time":1893456000}
        ]}}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.vendor, "minimax");
        assert_eq!(q.windows.len(), 2);
        let five = q.windows.iter().find(|w| w.label == "5h").unwrap();
        assert!((five.used_pct - 60.0).abs() < 1e-6);
        assert!(five.resets_at.is_some());
        let weekly = q.windows.iter().find(|w| w.label == "周").unwrap();
        assert!((weekly.used_pct - 85.0).abs() < 1e-6);
        // weekly 85% used → Danger
        assert_eq!(q.status, QuotaStatus::Danger);
    }

    #[test]
    fn parse_suppresses_placeholder_lane() {
        // weekly status==3 with 100% → suppressed; only 5h remains.
        let body = r#"{"data":{"model_remains":[
            {"model_name":"general",
             "current_interval_remaining_percent":80.0,
             "current_weekly_remaining_percent":100.0,"current_weekly_status":3}
        ]}}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "5h");
        assert!((q.windows[0].used_pct - 20.0).abs() < 1e-6);
    }

    #[test]
    fn parse_no_general_errors() {
        let body = r#"{"data":{"model_remains":[{"model_name":"video","current_interval_remaining_percent":50.0}]}}"#;
        assert!(matches!(parse(body), Err(VendorError::Empty)));
    }

    #[test]
    fn parse_top_level_array() {
        let body = r#"{"model_remains":[{"model_name":"general","current_interval_remaining_percent":70.0}]}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert!((q.windows[0].used_pct - 30.0).abs() < 1e-6);
    }
}
