//! DeepSeek balance adapter (T2.5 reference). Balance-type, API-key auth.
//!
//! `GET https://api.deepseek.com/user/balance` with `Authorization: Bearer <key>`
//! → `{ is_available, balance_infos: [{ currency, total_balance, granted_balance,
//! topped_up_balance }] }`.

use serde::Deserialize;

use super::types::{Quota, QuotaKind, QuotaStatus};
use super::VendorError;

const URL: &str = "https://api.deepseek.com/user/balance";

#[derive(Debug, Deserialize)]
struct Resp {
    #[serde(default)]
    balance_infos: Vec<BalanceInfo>,
}

#[derive(Debug, Deserialize)]
struct BalanceInfo {
    currency: String,
    total_balance: String,
}

/// HTTP client injected so the parse/normalize path is unit-testable.
pub trait Http {
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError>;
}

/// Parse a DeepSeek balance response into a [`Quota`]. Pure — tested directly.
pub fn parse(body: &str) -> Result<Quota, VendorError> {
    let resp: Resp = serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    let primary = resp
        .balance_infos
        .iter()
        .find(|b| b.currency == "CNY")
        .or_else(|| resp.balance_infos.first());
    let primary = match primary {
        Some(p) => p,
        None => return Err(VendorError::Empty),
    };
    let balance: f64 = primary
        .total_balance
        .parse()
        .map_err(|e| VendorError::Parse(format!("total_balance not a number: {e}")))?;
    // DeepSeek is prepaid; "low" if balance is small (heuristic — no fixed budget).
    let status = if balance < 1.0 {
        QuotaStatus::Danger
    } else if balance < 10.0 {
        QuotaStatus::Low
    } else {
        QuotaStatus::Ok
    };
    Ok(Quota {
        vendor: "deepseek".into(),
        kind: QuotaKind::Balance,
        status,
        value: Some(balance),
        display: format!(
            "{}{}",
            currency_symbol(&primary.currency),
            format_money(balance)
        ),
        reset_in_secs: None,
        used_pct: None,
        currency: Some(primary.currency.clone()),
    })
}

/// Fetch via `http`. Returns the normalized quota.
pub fn fetch_with(http: &dyn Http, api_key: &str) -> Result<Quota, VendorError> {
    super::validate_header_safe(api_key)?;
    let body = http.get(URL, api_key)?;
    parse(&body)
}

/// Default fetch (real network), called by the dispatch in super::fetch.
pub async fn fetch(api_key: &str) -> Result<Quota, VendorError> {
    tokio::task::spawn_blocking({
        let key = api_key.to_string();
        move || fetch_with(&UreqHttp, &key)
    })
    .await
    .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

fn currency_symbol(code: &str) -> &'static str {
    match code {
        "CNY" => "¥",
        "USD" => "$",
        _ => "",
    }
}

fn format_money(v: f64) -> String {
    format!("{v:.2}")
}

/// Default HTTP client (ureq, blocking, run via spawn_blocking).
struct UreqHttp;
impl Http for UreqHttp {
    fn get(&self, url: &str, bearer: &str) -> Result<String, VendorError> {
        let resp = ureq::get(url)
            .set("Authorization", &format!("Bearer {bearer}"))
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
    fn parse_cny_balance() {
        let body = r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"48.20","granted_balance":"50.00","topped_up_balance":"-1.80"}]}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.vendor, "deepseek");
        assert_eq!(q.kind, QuotaKind::Balance);
        assert_eq!(q.status, QuotaStatus::Ok);
        assert!((q.value.unwrap() - 48.20).abs() < 1e-9);
        assert_eq!(q.display, "¥48.20");
        assert_eq!(q.currency.as_deref(), Some("CNY"));
    }

    #[test]
    fn parse_low_and_danger_thresholds() {
        let low = parse(r#"{"balance_infos":[{"currency":"CNY","total_balance":"5"}]}"#).unwrap();
        assert_eq!(low.status, QuotaStatus::Low);
        let danger =
            parse(r#"{"balance_infos":[{"currency":"CNY","total_balance":"0.50"}]}"#).unwrap();
        assert_eq!(danger.status, QuotaStatus::Danger);
    }

    #[test]
    fn parse_empty_balances_errors() {
        assert!(matches!(
            parse(r#"{"balance_infos":[]}"#),
            Err(VendorError::Empty)
        ));
    }

    #[test]
    fn parse_malformed_errors() {
        assert!(matches!(parse("{not json"), Err(VendorError::Parse(_))));
    }

    #[test]
    fn fetch_with_uses_injected_http() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _url: &str, bearer: &str) -> Result<String, VendorError> {
                assert_eq!(bearer, "sk-test");
                Ok(r#"{"balance_infos":[{"currency":"CNY","total_balance":"100.00"}]}"#.into())
            }
        }
        let q = fetch_with(&Mock, "sk-test").unwrap();
        assert!((q.value.unwrap() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_crlf_and_empty_credentials() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                unreachable!("must not call http for invalid credential")
            }
        }
        assert!(fetch_with(&Mock, "").is_err());
        assert!(fetch_with(&Mock, "sk-bad\r\nX-Injected: yes").is_err());
        assert!(fetch_with(&Mock, "sk-bad\n").is_err());
    }
}
