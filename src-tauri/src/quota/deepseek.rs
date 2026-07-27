//! DeepSeek balance adapter (T2.5 reference). Balance-type, API-key auth.
//!
//! `GET https://api.deepseek.com/user/balance` with `Authorization: Bearer <key>`
//! → `{ is_available, balance_infos: [{ currency, total_balance, granted_balance,
//! topped_up_balance }] }`.

use serde::Deserialize;

use super::types::{Quota, QuotaBalance, QuotaStatus};
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
///
/// Row selection mirrors token-monitor's `selectFundedRow`: among rows with a
/// positive balance, take the largest (ties break toward USD); if none are
/// funded, fall back to the USD row, else the first.
pub fn parse(body: &str) -> Result<Quota, VendorError> {
    let resp: Resp = serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;

    // Parse rows into (currency, amount) pairs, dropping malformed ones.
    let mut rows: Vec<(String, f64)> = Vec::new();
    for info in &resp.balance_infos {
        if let Ok(amount) = info.total_balance.parse::<f64>() {
            if amount.is_finite() {
                rows.push((info.currency.clone(), amount));
            }
        }
    }
    if rows.is_empty() {
        return Err(VendorError::Empty);
    }

    // Prefer the largest funded row (ties → USD); else USD row; else first.
    let funded: Option<&(String, f64)> =
        rows.iter().filter(|(_, amt)| *amt > 0.0).max_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    // On equal amount, USD wins.
                    (b.0 == "USD").cmp(&(a.0 == "USD"))
                })
        });
    let (currency, balance) = funded
        .or_else(|| rows.iter().find(|(c, _)| c == "USD"))
        .or_else(|| rows.first())
        .map(|(c, a)| (c.clone(), *a))
        .ok_or(VendorError::Empty)?;

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
        plan_label: Some("Pay-as-you-go".into()),
        status,
        windows: vec![],
        balance: Some(QuotaBalance {
            amount: balance,
            currency,
            today_consumption: None,
            month_consumption: None,
        }),
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
    })
}

/// Fetch via `http`. Returns the normalized quota.
pub fn fetch_with(http: &dyn Http, api_key: &str) -> Result<Quota, VendorError> {
    // Credential may be JSON `{"key":"sk-..."}` or a plain key string.
    let key = super::extract_key(api_key);
    super::validate_header_safe(&key)?;
    let body = http.get(URL, &key)?;
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

/// Default HTTP client (ureq, blocking, run via spawn_blocking).
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
    fn parse_cny_balance() {
        let body = r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"48.20","granted_balance":"50.00","topped_up_balance":"-1.80"}]}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.vendor, "deepseek");
        assert_eq!(q.status, QuotaStatus::Ok);
        assert!(q.windows.is_empty());
        let bal = q.balance.unwrap();
        assert!((bal.amount - 48.20).abs() < 1e-9);
        assert_eq!(bal.currency, "CNY");
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
    fn parse_selects_largest_funded_row() {
        // Two rows: CNY 0 (unfunded) + USD 20 (funded) → pick the funded USD row.
        let body = r#"{"balance_infos":[
            {"currency":"CNY","total_balance":"0.00"},
            {"currency":"USD","total_balance":"20.00"}
        ]}"#;
        let q = parse(body).unwrap();
        let bal = q.balance.unwrap();
        assert_eq!(bal.currency, "USD");
        assert!((bal.amount - 20.0).abs() < 1e-9);
    }

    #[test]
    fn parse_falls_back_to_usd_when_none_funded() {
        // Both unfunded → fall back to the USD row.
        let body = r#"{"balance_infos":[
            {"currency":"CNY","total_balance":"0.00"},
            {"currency":"USD","total_balance":"0.00"}
        ]}"#;
        let q = parse(body).unwrap();
        assert_eq!(q.balance.unwrap().currency, "USD");
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
        assert!((q.balance.unwrap().amount - 100.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_crlf_and_empty_credentials() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                unreachable!("must not call http for invalid credential")
            }
        }
        // Empty credential (after trimming JSON wrapper) is rejected.
        assert!(fetch_with(&Mock, "").is_err());
        assert!(fetch_with(&Mock, r#"{"key":""}"#).is_err());
        // CRLF injection in the middle of the key is rejected.
        assert!(fetch_with(&Mock, "sk-bad\r\nX-Injected: yes").is_err());
        assert!(fetch_with(&Mock, r#"{"key":"sk-bad\r\nX-Injected: yes"}"#).is_err());
    }
}
