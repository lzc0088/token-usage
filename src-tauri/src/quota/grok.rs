//! Grok / xAI quota adapter.
//!
//! `GET https://api.x.ai/v1/usage` with `Authorization: Bearer <key>`
//! → `{ usage: { prompt_tokens, completion_tokens, total_tokens } }`

use serde::Deserialize;

use super::types::{Quota, QuotaBalance, QuotaStatus, QuotaWindow};
use super::VendorError;

const URL: &str = "https://api.x.ai/v1/usage";

pub trait Http {
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError>;
}

#[derive(Debug, Deserialize)]
struct XaiUsage {
    usage: XaiSummary,
}
#[derive(Debug, Deserialize)]
struct XaiSummary {
    #[serde(default)]
    total_tokens: i64,
}

pub fn parse(body: &str) -> Result<Quota, VendorError> {
    let u: XaiUsage =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(format!("xAI: {e}")))?;
    let total = u.usage.total_tokens as f64;
    Ok(Quota {
        vendor: "grok".into(),
        status: QuotaStatus::Ok,
        windows: vec![QuotaWindow {
            label: "Tokens used".into(),
            used_pct: 0.0,
            resets_at: None,
            used_value: Some(total),
            total_value: None,
            sub_items: None,
        }],
        balance: Some(QuotaBalance {
            amount: total,
            currency: "tokens".into(),
            today_consumption: None,
            month_consumption: None,
        }),
        plan_label: Some("xAI".into()),
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
            .map_err(|e| VendorError::Network(format!("xAI: {e}")))?;
        let status = resp.status();
        if status == 401 || status == 403 {
            return Err(VendorError::Auth(format!(
                "xAI API key rejected (HTTP {status})"
            )));
        }
        resp.into_string()
            .map_err(|e| VendorError::Network(format!("read xAI body: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeHttp(String);
    impl Http for FakeHttp {
        fn get(&self, _url: &str, _bearer: &str) -> Result<String, VendorError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn parse_usage() {
        let json = r#"{"usage":{"total_tokens":150}}"#;
        let q = parse(json).unwrap();
        assert_eq!(q.vendor, "grok");
        assert!((q.balance.unwrap().amount - 150.0).abs() < 1e-9);
    }
}
