//! Uniform quota VM shared by all vendor adapters (T2.5).
//!
//! v2: a vendor's quota is a set of **subscription windows** (e.g. GLM's
//! 5-hour / weekly / MCP-monthly) plus an optional **balance** (prepaid credit,
//! e.g. DeepSeek). Each adapter fills whichever the vendor reports.

use serde::{Deserialize, Serialize};

/// Convert a vendor reset marker to an RFC3339 string.
///
/// Accepts epoch seconds or milliseconds (heuristic: < 2e10 ⇒ seconds, matching
/// token-monitor's `toIso`). Values ≤ 0 or non-finite return `None`.
pub fn epoch_to_iso(value: f64) -> Option<String> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    let millis = if value < 20_000_000_000.0 {
        (value * 1000.0) as i64
    } else {
        value as i64
    };
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis).map(|dt| dt.to_rfc3339())
}

/// Parse an already-ISO reset string (validated) or `None` if unparseable/empty.
pub fn parse_iso(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(trimmed)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339())
}

/// Vendor-normalized quota. Frontend renders one shape regardless of vendor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    pub vendor: String,
    /// Overall traffic-light status: worst window's status, or balance-derived.
    pub status: QuotaStatus,
    /// Subscription / plan windows (label + used%). Empty for balance-only vendors.
    pub windows: Vec<QuotaWindow>,
    /// Prepaid balance (amount + currency). None for window-only vendors.
    pub balance: Option<QuotaBalance>,
    /// Plan / account label (e.g. "Pay-as-you-go", "Pro", "Token Plan").
    /// Displayed below the vendor name when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_label: Option<String>,
    /// Server timestamp (RFC3339) when the data was fetched.
    /// Frontend uses this to show "N分钟前刷新".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<String>,
    /// Non-empty when the last fetch failed with a user-actionable error
    /// (e.g. "凭证已失效"). Frontend displays this prominently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Non-empty when an OPTIONAL cookie (used only for plan name / expiry)
    /// has expired while usage data is still fine — e.g. Volcengine, whose
    /// API-Key path still works but the console cookie for EndTime/tier is
    /// dead. Frontend shows an inline "Cookie 已过期" hint at the affected
    /// position (plan/expiry slot) with an update entry, and keeps rendering
    /// usage windows. Cookie-only vendors (mimo/stepfun/kimi) also use this
    /// field — their whole card has no data, so the hint shows centrally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_error: Option<String>,
    /// Subscription plan expiry (RFC3339), distinct from per-window reset time.
    /// A window's `resets_at` is the rolling quota reset (e.g. "5h resets in
    /// 3h"); this is when the whole PLAN ends. Frontend shows it as the
    /// "到期" tag. `None` for balance-only / no-plan vendors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Region / site identifier for multi-region vendors (e.g. Qoder "cn"/"global").
    /// Used to construct the correct console URL when opening the vendor panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
}

/// One subscription window (e.g. "5h 42%", "weekly 78%", "MCP 月 25%").
///
/// For vendors with multiple individual quota items (e.g. Qoder's per-package
/// resource_package entries), the aggregate window carries `used_value` /
/// `total_value` for the summary "X / Y credits" display, and `sub_items`
/// lists each individual item with its own expiry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct QuotaWindow {
    /// Human label, e.g. "5h", "周", "MCP 月", "monthly", "订阅", "资源包".
    pub label: String,
    /// Used percentage 0..100 (from the summary bucket).
    pub used_pct: f64,
    /// Absolute reset time as an RFC3339/ISO-8601 string, parsed from the
    /// vendor's real API response. `None` when the vendor doesn't report it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    /// Summary used value (e.g. credits consumed). `None` when not applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub used_value: Option<f64>,
    /// Summary total value (e.g. credits limit). `None` when not applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub total_value: Option<f64>,
    /// Individual quota items within this window (e.g. each resource package).
    /// When present, the frontend renders one row per item instead of a single
    /// aggregate progress bar.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub sub_items: Option<Vec<QuotaWindowSubItem>>,
    /// Projected absolute exhaustion time (RFC3339) computed by the burn-rate
    /// tracker on the last successful probe. `None` when the rate is too stale,
    /// zero, or the window is already exhausted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub projected_exhaustion_at: Option<String>,
}

/// An individual quota item within a window (e.g. one resource package bonus).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct QuotaWindowSubItem {
    /// Item label (e.g. "Bonus Pack #1"). Empty for default display.
    pub name: String,
    /// Used value (credits consumed).
    pub used: f64,
    /// Total value (credits limit).
    pub total: f64,
    /// Used percentage 0..100.
    pub pct: f64,
    /// Item-level expiry/reset time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Prepaid balance (DeepSeek, MiMo, future top-up vendors).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaBalance {
    pub amount: f64,
    /// ISO currency code, e.g. "CNY", "USD".
    pub currency: String,
    /// Today's consumption (from local usage tracking). `None` if unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub today_consumption: Option<f64>,
    /// Month's consumption (from local usage tracking). `None` if unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month_consumption: Option<f64>,
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

    /// Worst (most severe) of a set — drives the overall `Quota.status`.
    pub fn worst_of(statuses: impl IntoIterator<Item = QuotaStatus>) -> Self {
        let mut worst = QuotaStatus::Ok;
        for s in statuses {
            if matches!(s, QuotaStatus::Danger) {
                return QuotaStatus::Danger;
            }
            if matches!(s, QuotaStatus::Low) {
                worst = QuotaStatus::Low;
            }
        }
        worst
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
    fn worst_of_picks_most_severe() {
        assert_eq!(
            QuotaStatus::worst_of([QuotaStatus::Ok, QuotaStatus::Low, QuotaStatus::Ok]),
            QuotaStatus::Low
        );
        assert_eq!(
            QuotaStatus::worst_of([QuotaStatus::Ok, QuotaStatus::Danger, QuotaStatus::Low]),
            QuotaStatus::Danger
        );
        assert_eq!(QuotaStatus::worst_of([]), QuotaStatus::Ok);
    }

    #[test]
    fn windows_quota_serializes() {
        let q = Quota {
            site: None,
            vendor: "glm".into(),
            status: QuotaStatus::Low,
            windows: vec![
                QuotaWindow {
                    label: "5h".into(),
                    used_pct: 42.0,
                    resets_at: None,
                    ..Default::default()
                },
                QuotaWindow {
                    label: "周".into(),
                    used_pct: 78.0,
                    resets_at: None,
                    ..Default::default()
                },
            ],
            balance: None,
            plan_label: None,
            refreshed_at: None,
            error: None,
            cookie_error: None,
            expires_at: None,
        };
        let s = serde_json::to_string(&q).unwrap();
        assert!(s.contains("\"status\":\"low\""));
        assert!(s.contains("\"label\":\"5h\""));
        assert!(s.contains("\"used_pct\":42.0"));
        let back: Quota = serde_json::from_str(&s).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn balance_quota_serializes() {
        let q = Quota {
            site: None,
            vendor: "deepseek".into(),
            status: QuotaStatus::Ok,
            windows: vec![],
            balance: Some(QuotaBalance {
                amount: 48.2,
                currency: "CNY".into(),
                today_consumption: None,
                month_consumption: None,
            }),
            plan_label: None,
            refreshed_at: None,
            error: None,
            cookie_error: None,
            expires_at: None,
        };
        let s = serde_json::to_string(&q).unwrap();
        assert!(s.contains("\"balance\":{\"amount\":48.2,\"currency\":\"CNY\"}"));
    }
}
