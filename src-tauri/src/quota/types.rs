//! Uniform quota VM shared by all vendor adapters (T2.5).

use serde::{Deserialize, Serialize};

/// Vendor-normalized quota. Frontend renders one shape regardless of vendor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    pub vendor: String,
    pub kind: QuotaKind,
    pub status: QuotaStatus,
    /// For balance types: current balance in the vendor's native currency.
    /// For plan types: remaining tokens (or None if the vendor only reports %).
    pub value: Option<f64>,
    /// Human-readable value (e.g. "¥48.20" / "62%") — pre-formatted for the UI.
    pub display: String,
    /// For plan/window types: seconds until the window resets (None = N/A).
    pub reset_in_secs: Option<i64>,
    /// For plan types: used percentage (0..100). Drives the progress bar + status.
    pub used_pct: Option<f64>,
    /// ISO currency code when value is monetary (CNY/USD), else None.
    pub currency: Option<String>,
}

/// How this vendor's quota is expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaKind {
    /// Pre-paid balance (deepseek, volcengine top-up). value = balance.
    Balance,
    /// Subscription window / quota (claude 5h/weekly, codex, minimax). used_pct drives UI.
    Plan,
}

/// Traffic-light status, derived from used_pct / balance thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaStatus {
    /// Normal.
    Ok,
    /// <50% remaining.
    Low,
    /// <20% remaining.
    Danger,
}

impl QuotaStatus {
    /// Derive plan-window status from used percentage (higher used = worse).
    pub fn from_used_pct(used_pct: f64) -> Self {
        if used_pct >= 80.0 {
            QuotaStatus::Danger
        } else if used_pct >= 50.0 {
            QuotaStatus::Low
        } else {
            QuotaStatus::Ok
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_thresholds() {
        assert_eq!(QuotaStatus::from_used_pct(10.0), QuotaStatus::Ok);
        assert_eq!(QuotaStatus::from_used_pct(50.0), QuotaStatus::Low);
        assert_eq!(QuotaStatus::from_used_pct(79.9), QuotaStatus::Low);
        assert_eq!(QuotaStatus::from_used_pct(80.0), QuotaStatus::Danger);
        assert_eq!(QuotaStatus::from_used_pct(99.0), QuotaStatus::Danger);
    }

    #[test]
    fn quota_serializes_for_frontend() {
        let q = Quota {
            vendor: "deepseek".into(),
            kind: QuotaKind::Balance,
            status: QuotaStatus::Ok,
            value: Some(48.2),
            display: "¥48.20".into(),
            reset_in_secs: None,
            used_pct: None,
            currency: Some("CNY".into()),
        };
        let s = serde_json::to_string(&q).unwrap();
        assert!(s.contains("\"kind\":\"balance\""));
        assert!(s.contains("\"status\":\"ok\""));
        let back: Quota = serde_json::from_str(&s).unwrap();
        assert_eq!(back, q);
    }
}
