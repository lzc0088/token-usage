//! OpenRouter quota adapter.
//!
//! `GET https://openrouter.ai/api/v1/auth/key` with `Authorization: Bearer <key>`
//! → `{ data: { label, limit, usage, limit_remaining, is_free_tier } }`

use serde::Deserialize;

use super::types::{Quota, QuotaBalance, QuotaStatus, QuotaWindow};
use super::VendorError;

const URL: &str = "https://openrouter.ai/api/v1/auth/key";

pub trait Http {
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError>;
}

#[derive(Debug, Deserialize)]
struct OrResponse {
    data: OrData,
}
#[derive(Debug, Deserialize)]
struct OrData {
    #[serde(default)]
    #[allow(dead_code)]
    label: String,
    limit: Option<f64>,
    #[serde(default)]
    usage: f64,
    limit_remaining: Option<f64>,
    #[serde(default)]
    is_free_tier: bool,
}

pub fn parse(body: &str) -> Result<Quota, VendorError> {
    let d: OrResponse =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(format!("OpenRouter: {e}")))?;
    let used = d.data.usage;
    let limit = d.data.limit.unwrap_or(0.0);
    let remaining = d.data.limit_remaining.unwrap_or(0.0);
    let status = if d.data.is_free_tier || limit <= 0.0 {
        QuotaStatus::Ok
    } else if remaining < limit * 0.1 {
        QuotaStatus::Low
    } else if remaining <= 0.0 {
        QuotaStatus::Danger
    } else {
        QuotaStatus::Ok
    };
    let window = QuotaWindow {
        label: if d.data.is_free_tier {
            "Credits (Free)".into()
        } else {
            "Credits".into()
        },
        used_pct: if limit > 0.0 {
            (used / limit * 100.0).min(100.0)
        } else {
            0.0
        },
        resets_at: None,
        used_value: Some(used),
        total_value: if limit > 0.0 { Some(limit) } else { None },
        sub_items: None,
    };
    Ok(Quota {
        vendor: "openrouter".into(),
        status,
        windows: vec![window],
        balance: Some(QuotaBalance {
            amount: remaining,
            currency: "USD".into(),
            today_consumption: None,
            month_consumption: None,
        }),
        plan_label: if d.data.is_free_tier {
            Some("Free Tier".into())
        } else {
            None
        },
        refreshed_at: Some(chrono::Utc::now().to_rfc3339()),
        error: None,
        cookie_error: None,
        expires_at: None,
    })
}

pub fn fetch_with(http: &dyn Http, api_key: &str) -> Result<Quota, VendorError> {
    let key = super::extract_key(api_key);
    super::validate_header_safe(&key)?;
    let body = http.get(URL, &key)?;
    parse(&body)
}

pub async fn fetch(api_key: &str) -> Result<Quota, VendorError> {
    tokio::task::spawn_blocking({
        let key = api_key.to_string();
        move || fetch_with(&UreqHttp, &key)
    })
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
            .map_err(|e| VendorError::Network(format!("OpenRouter: {e}")))?;
        let status = resp.status();
        if status == 401 || status == 403 {
            return Err(VendorError::Auth(format!(
                "OpenRouter API key rejected (HTTP {status})"
            )));
        }
        resp.into_string()
            .map_err(|e| VendorError::Network(format!("read OpenRouter body: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]

    struct FakeHttp(String);
    impl Http for FakeHttp {
        fn get(&self, _url: &str, _bearer: &str) -> Result<String, VendorError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn parse_key_info() {
        let json = r#"{"data":{"label":"My Key","limit":100.0,"usage":42.5,"limit_remaining":57.5,"is_free_tier":false}}"#;
        let q = parse(json).unwrap();
        assert_eq!(q.vendor, "openrouter");
        assert_eq!(q.status, QuotaStatus::Ok);
        assert!((q.balance.unwrap().amount - 57.5).abs() < 1e-9);
    }

    #[test]
    fn parse_free_tier() {
        let json = r#"{"data":{"label":"Free","limit":null,"usage":15.0,"limit_remaining":null,"is_free_tier":true}}"#;
        let q = parse(json).unwrap();
        assert!(q.plan_label.unwrap().contains("Free"));
        assert!(q.windows[0].total_value.is_none());
    }
}
